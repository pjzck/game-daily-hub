use crate::accounts::{Game, now};
use chrono::{DateTime, Datelike, FixedOffset, NaiveDate, NaiveDateTime, TimeZone};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::HashSet, sync::LazyLock, time::Duration};

const SRA: &str = "https://starrailassistant.top/api/v1/activity";
const WISHES: &str =
    "https://raw.githubusercontent.com/UIGF-org/CurrentBannerWatcher/main/banner-data.json";
const WARPS: &str =
    "https://hkrpg-api-static.mihoyo.com/common/hkrpg_cn/announcement/api/getAnnContent";
const HEADHUNTING: &str = "https://raw.githubusercontent.com/Kengxxiao/ArknightsGameData/master/zh_CN/gamedata/excel/gacha_table.json";
const TAPTAP: &str = "https://www.taptap.cn/app/221062/topic?type=official";
const DAY: i64 = 86_400;
const LIMIT: usize = 3 * 1024 * 1024;
const NO_FORMAT: &str = "无法识别日程来源的数据格式。";

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Activity,
    Pool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Reminder {
    title: String,
    start_at: Option<i64>,
    end_at: i64,
}

#[derive(Serialize)]
pub struct Feed {
    game: Game,
    kind: Kind,
    entries: Vec<Reminder>,
    error: Option<String>,
}

impl Feed {
    fn new(game: Game, kind: Kind, result: Result<Vec<Reminder>, String>, timestamp: i64) -> Self {
        let (mut entries, error) = match result {
            Ok(entries) => (entries, None),
            Err(reason) => (Vec::new(), Some(reason)),
        };
        entries.retain(|entry| {
            entry.end_at > timestamp
                && entry.end_at <= timestamp + 150 * DAY
                && entry.start_at.is_none_or(|start| start < entry.end_at)
        });
        entries.sort_by_key(|entry| entry.end_at);
        let mut seen = HashSet::new();
        entries.retain(|entry| seen.insert((entry.title.clone(), entry.end_at)));
        Self {
            game,
            kind,
            entries,
            error,
        }
    }
}

#[tauri::command]
pub async fn fetch_reminders() -> Result<Vec<Feed>, String> {
    let client = Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .user_agent("GameDailyHub/0.1 (public game schedules)")
        .build()
        .map_err(|_| "无法初始化日程连接。")?;
    Ok(fetch(&client).await)
}

async fn fetch(client: &Client) -> Vec<Feed> {
    let timestamp = now() as i64;
    let mut jobs = Vec::new();
    for (game, slug) in [
        (Game::Genshin, "ys"),
        (Game::Starrail, "sr"),
        (Game::Arknights, "ak"),
    ] {
        let client = client.clone();
        jobs.push(tauri::async_runtime::spawn(async move {
            let result = download(client.get(format!("{SRA}/{slug}.json")))
                .await
                .and_then(|body| activities(&body));
            Feed::new(game, Kind::Activity, result, timestamp)
        }));
    }
    for game in [Game::Genshin, Game::Starrail, Game::Arknights] {
        let client = client.clone();
        jobs.push(tauri::async_runtime::spawn(async move {
            let result = match game {
                Game::Genshin => download(client.get(WISHES))
                    .await
                    .and_then(|body| wishes(&body)),
                Game::Starrail => {
                    let request = client.get(WARPS).query(&[
                        ("game", "hkrpg"),
                        ("game_biz", "hkrpg_cn"),
                        ("lang", "zh-cn"),
                        ("bundle_id", "hkrpg_cn"),
                        ("platform", "pc"),
                        ("region", "prod_gf_cn"),
                        ("level", "30"),
                        ("uid", "11111111"),
                    ]);
                    download(request).await.and_then(|body| warps(&body))
                }
                Game::Arknights => download(client.get(HEADHUNTING))
                    .await
                    .and_then(|body| headhunting(&body)),
                _ => unreachable!(),
            };
            Feed::new(game, Kind::Pool, result, timestamp)
        }));
    }
    let reverse_jobs: Vec<_> = (1..=4).map(|page| {
        let client = client.clone();
        tauri::async_runtime::spawn(async move {
            let request = client.get(TAPTAP)
                .header(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/131.0.0.0 Safari/537.36");
            let request = if page == 1 { request } else { request.query(&[("page", page)]) };
            download(request).await.and_then(|body| reverse_schedule(&body))
        })
    }).collect();

    let mut feeds = Vec::new();
    for job in jobs {
        if let Ok(feed) = job.await {
            feeds.push(feed);
        }
    }
    let mut reverse_activities = Vec::new();
    let mut reverse_pools = Vec::new();
    let mut missing_page = false;
    for job in reverse_jobs {
        match job.await {
            Ok(Ok(page)) => {
                for (kind, entry) in page {
                    match kind {
                        Kind::Activity => reverse_activities.push(entry),
                        Kind::Pool => reverse_pools.push(entry),
                    }
                }
            }
            _ => missing_page = true,
        }
    }
    for (kind, entries) in [
        (Kind::Activity, reverse_activities),
        (Kind::Pool, reverse_pools),
    ] {
        let mut feed = Feed::new(Game::Reverse1999, kind, Ok(entries), timestamp);
        if missing_page {
            feed.error = Some("部分 TapTap 公告未能读取。".into());
        }
        feeds.push(feed);
    }
    feeds
}

async fn download(request: reqwest::RequestBuilder) -> Result<String, String> {
    let mut response = request
        .send()
        .await
        .map_err(|_| "无法连接日程来源。")?
        .error_for_status()
        .map_err(|error| {
            format!(
                "日程来源响应异常（{}）。",
                error.status().map_or(0, |status| status.as_u16())
            )
        })?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| "日程下载中断。")? {
        if bytes.len() + chunk.len() > LIMIT {
            return Err("日程数据超过读取上限。".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    String::from_utf8(bytes).map_err(|_| NO_FORMAT.into())
}

fn china() -> FixedOffset {
    FixedOffset::east_opt(8 * 3_600).expect("UTC+8 is valid")
}

fn china_time(value: &str) -> Option<i64> {
    if let Ok(date) = DateTime::parse_from_rfc3339(value) {
        return Some(date.timestamp());
    }
    [
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S",
        "%Y/%m/%d %H:%M:%S",
        "%Y/%m/%d %H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(value.trim(), format).ok())
    .and_then(|date| china().from_local_datetime(&date).single())
    .map(|date| date.timestamp())
}

fn parse<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T, String> {
    serde_json::from_str(body.trim_start_matches('\u{feff}')).map_err(|_| NO_FORMAT.into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivityRow {
    name: String,
    start_time: String,
    end_time: String,
}

fn activities(body: &str) -> Result<Vec<Reminder>, String> {
    #[derive(Deserialize)]
    struct Schedule {
        activities: Vec<ActivityRow>,
    }
    let feed: Schedule = parse(body)?;
    let entries: Vec<_> = feed
        .activities
        .iter()
        .filter_map(|row| {
            Some(Reminder {
                title: row.name.clone(),
                start_at: Some(china_time(&row.start_time)?),
                end_at: china_time(&row.end_time)?,
            })
        })
        .collect();
    if entries.is_empty() && !feed.activities.is_empty() {
        return Err(NO_FORMAT.into());
    }
    Ok(entries)
}

fn wishes(body: &str) -> Result<Vec<Reminder>, String> {
    let data: Value = parse(body)?;
    let pools = data.as_object().ok_or(NO_FORMAT)?;
    let entries: Vec<_> = pools
        .values()
        .filter_map(|row| {
            Some(Reminder {
                title: row["zh-cn"]["banner_name"].as_str()?.to_owned(),
                start_at: Some(china_time(row["start_time"].as_str()?)?),
                end_at: china_time(row["end_time"].as_str()?)?,
            })
        })
        .collect();
    if entries.is_empty() && !pools.is_empty() {
        return Err(NO_FORMAT.into());
    }
    Ok(entries)
}

fn headhunting(body: &str) -> Result<Vec<Reminder>, String> {
    let data: Value = parse(body)?;
    let rows = data["gachaPoolClient"].as_array().ok_or(NO_FORMAT)?;
    Ok(rows
        .iter()
        .filter_map(|row| {
            let rule = row["gachaRuleType"].as_str()?;
            if rule == "BACKFLOW" {
                return None;
            }
            let name = row["gachaPoolName"].as_str()?;
            let title = if name == "适合多种场合的强力干员" {
                match rule {
                    "CLASSIC" | "CLASSIC_DOUBLE" | "FESCLASSIC" => "中坚寻访",
                    "DOUBLE" => "常驻标准寻访轮换",
                    "GACHAWITHSPECIFIED6" => "自选六星寻访",
                    _ => "限时寻访",
                }
            } else {
                name
            };
            Some(Reminder {
                title: title.into(),
                start_at: Some(row["openTime"].as_i64()?),
                end_at: row["endTime"].as_i64()?,
            })
        })
        .collect())
}

static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^<>]*>").unwrap());
static WARP_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"「([^「」\r\n]{1,85})」\s*(?:角色|光锥|联动)*活动跃迁期间[，,]").unwrap()
});
static WARP_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{4}/\d{1,2}/\d{1,2}\s+\d{1,2}:\d{2}(?::\d{2})?|\d+(?:\.\d+)?版本更新后)\s*[-－–—~～至]\s*(\d{4}/\d{1,2}/\d{1,2}\s+\d{1,2}:\d{2}(?::\d{2})?)").unwrap()
});

fn warps(body: &str) -> Result<Vec<Reminder>, String> {
    let data: Value = parse(body)?;
    if data["retcode"].as_i64() != Some(0) {
        return Err(NO_FORMAT.into());
    }
    let data = &data["data"];
    let mut entries = Vec::new();
    let mut has_warps = false;
    for key in ["list", "pic_list"] {
        let posts = data[key].as_array().ok_or(NO_FORMAT)?;
        for post in posts {
            let Some(title) = post["title"]
                .as_str()
                .filter(|title| title.contains("活动跃迁"))
            else {
                continue;
            };
            let Some(body) = post["content"].as_str() else {
                continue;
            };
            has_warps = true;
            let plain = body
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&nbsp;", " ")
                .replace('\u{a0}', " ");
            let plain = HTML_TAG.replace_all(&plain, " ");
            let mut headings = WARP_HEADING.captures_iter(&plain).peekable();
            let old_count = entries.len();
            while let Some(heading) = headings.next() {
                let end = headings
                    .peek()
                    .map_or(plain.len(), |next| next.get(0).unwrap().start());
                let section = &plain[heading.get(0).unwrap().end()..end];
                let Some((_, period)) = section.split_once("活动时间") else {
                    continue;
                };
                let Some(period) = WARP_RANGE
                    .captures(period.chars().take(220).collect::<String>().as_str())
                    .and_then(|range| warp_period(&range))
                else {
                    continue;
                };
                entries.push(Reminder {
                    title: heading[1].trim().into(),
                    start_at: period.0,
                    end_at: period.1,
                });
            }
            if entries.len() == old_count {
                let mut seen = HashSet::new();
                for range in WARP_RANGE.captures_iter(&plain).take(4) {
                    if let Some((start_at, end_at)) = warp_period(&range)
                        && seen.insert(end_at)
                    {
                        entries.push(Reminder {
                            title: title.into(),
                            start_at,
                            end_at,
                        });
                    }
                }
            }
        }
    }
    if entries.is_empty() && has_warps {
        return Err(NO_FORMAT.into());
    }
    Ok(entries)
}

fn warp_period(range: &regex::Captures<'_>) -> Option<(Option<i64>, i64)> {
    Some((china_time(&range[1]), china_time(&range[2])?))
}

static TAP_TIME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"▶+\s*(活动(?:开放|开启)?时间|征集(?:开放|开启)?时间)\s*◀+\s*([^▶◀]+)").unwrap()
});
const TAP_DATE_PATTERN: &str =
    r"(?:(\d{4})[/.年-])?(\d{1,2})[/.月-](\d{1,2})日?\s*(\d{1,2})[:：](\d{2})(?::(\d{2}))?";
static TAP_DATE: LazyLock<Regex> = LazyLock::new(|| Regex::new(TAP_DATE_PATTERN).unwrap());
static TAP_RANGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
    &format!(r"(?P<start>{TAP_DATE_PATTERN}|(?:版本)?(?:更新|维护)(?:完成)?后)\s*[-－—–~～至]\s*(?P<end>{TAP_DATE_PATTERN})")
).unwrap()
});
static TAP_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[「【〖]([^」】〗\n]{1,80})[」】〗]").unwrap());

fn reverse_schedule(html: &str) -> Result<Vec<(Kind, Reminder)>, String> {
    let (_, rest) = html.split_once("id=\"__NUXT_DATA__\"").ok_or(NO_FORMAT)?;
    let (_, rest) = rest.split_once('>').ok_or(NO_FORMAT)?;
    let (payload, _) = rest.split_once("</script>").ok_or(NO_FORMAT)?;
    let rows: Vec<Value> = parse(payload)?;
    let reference = |value: &Value| rows.get(value.as_u64()? as usize);
    let mut entries = Vec::new();
    let mut official_posts = 0;
    for row in &rows {
        if reference(&row["is_official"]).and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(topic) = reference(&row["topic"]) else {
            continue;
        };
        let Some(title) = reference(&topic["title"]).and_then(Value::as_str) else {
            continue;
        };
        let Some(summary) = reference(&topic["summary"]).and_then(Value::as_str) else {
            continue;
        };
        let Some(published) = reference(&row["publish_time"]).and_then(Value::as_i64) else {
            continue;
        };
        official_posts += 1;
        for capture in TAP_TIME.captures_iter(summary) {
            let kind = if capture[1].starts_with("征集") {
                Kind::Pool
            } else {
                Kind::Activity
            };
            let period = capture[2].trim();
            let Some((start, end)) = tap_period(period, published) else {
                continue;
            };
            let prefix = TAP_RANGE
                .find(period)
                .map_or("", |dates| &period[..dates.start()]);
            let name = TAP_NAME
                .captures(prefix)
                .or_else(|| TAP_NAME.captures(title))
                .map_or(title, |name| name.get(1).unwrap().as_str());
            entries.push((
                kind,
                Reminder {
                    title: name.trim().to_owned(),
                    start_at: start,
                    end_at: end,
                },
            ));
        }
    }
    if official_posts == 0 {
        return Err(NO_FORMAT.into());
    }
    Ok(entries)
}

fn tap_period(period: &str, published: i64) -> Option<(Option<i64>, i64)> {
    let parts = TAP_RANGE.captures(period)?;
    let start = TAP_DATE
        .captures(&parts["start"])
        .and_then(|date| tap_date(&date, published));
    let end = tap_date(&TAP_DATE.captures(&parts["end"])?, published)?;
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taptap_dates_use_beijing_time_and_handle_new_year() {
        let published = china_time("2026-12-30T12:00:00").unwrap();
        assert_eq!(
            tap_period("12/31 5:00 - 1/15 4:59", published),
            Some((
                Some(china_time("2026-12-31T05:00:00").unwrap()),
                china_time("2027-01-15T04:59:00").unwrap()
            ))
        );
        assert_eq!(
            tap_period("版本更新后—2027-01-15 4:59", published),
            Some((None, china_time("2027-01-15T04:59:00").unwrap()))
        );
        assert!(tap_period("12/31 - 1/15", published).is_none());
    }

    #[test]
    fn warp_pools_in_the_same_announcement_keep_their_own_deadlines() {
        let payload = serde_json::json!({"retcode": 0, "data": {
            "list": [], "pic_list": [{"title": "版本活动跃迁", "content":
                "<p>「甲」角色活动跃迁期间，</p><table><tr><td>活动时间</td></tr><tr><td>4.7版本更新后 - &lt;t&gt;2026/12/22 15:00:00&lt;/t&gt;</td></tr></table><p>「乙」光锥活动跃迁期间，</p><table><tr><td>活动时间</td></tr><tr><td>2026/11/11 11:00:00 - 2026/12/02 11:59:00</td></tr></table>"
            }]
        }});
        let entries = warps(&payload.to_string()).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "甲");
        assert_eq!(
            entries[0].end_at,
            china_time("2026-12-22T15:00:00").unwrap()
        );
        assert_eq!(entries[1].title, "乙");
        assert_eq!(
            entries[1].end_at,
            china_time("2026-12-02T11:59:00").unwrap()
        );
    }
}

fn tap_date(date: &regex::Captures<'_>, published: i64) -> Option<i64> {
    let fallback = china().timestamp_opt(published, 0).single()?.year();
    let year = date
        .get(1)
        .and_then(|year| year.as_str().parse::<i32>().ok());
    let month = date[2].parse().ok()?;
    let day = date[3].parse().ok()?;
    let hour = date[4].parse().ok()?;
    let minute = date[5].parse().ok()?;
    let second = date
        .get(6)
        .and_then(|sec| sec.as_str().parse().ok())
        .unwrap_or(0);
    [fallback, fallback + 1, fallback - 1]
        .into_iter()
        .filter(|&candidate| year.is_none_or(|year| candidate == year))
        .filter_map(|year| {
            let time =
                NaiveDate::from_ymd_opt(year, month, day)?.and_hms_opt(hour, minute, second)?;
            Some(china().from_local_datetime(&time).single()?.timestamp())
        })
        .min_by_key(|&timestamp| timestamp.abs_diff(published))
}
