use crate::{
    accounts::Game,
    community::{response, retcode, valid_cookie_value},
    gacha::{self, Pull},
};
use reqwest::{Client, Url};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
};

const HISTORY_HOST: &str = "public-operation-hkrpg.mihoyo.com";
const HISTORY_PAGE: &str = "/hkrpg/event/e20211215gacha-v2/index.html";
const STANDARD_API: &str = "/common/hkrpg_gacha_record/api/getGachaLog";
const COLLAB_API: &str = "/common/hkrpg_gacha_record/api/getLdGachaLog";
const OPEN_HISTORY: &str =
    "请在崩铁 PC 客户端打开对应账号的「跃迁 → 历史记录」，等列表加载后再试。";
const POOLS: [(&str, &str, &str); 6] = [
    ("11", "角色活动跃迁", STANDARD_API),
    ("12", "光锥活动跃迁", STANDARD_API),
    ("1", "常驻跃迁", STANDARD_API),
    ("2", "始发跃迁", STANDARD_API),
    ("21", "角色联动跃迁", COLLAB_API),
    ("22", "光锥联动跃迁", COLLAB_API),
];

pub(crate) struct History {
    timestamp: u64,
    params: BTreeMap<String, String>,
}

fn parse_url(raw: &str) -> Option<History> {
    let url = Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return None;
    }
    match (url.host_str()?, url.path()) {
        ("webstatic.mihoyo.com", HISTORY_PAGE) | (HISTORY_HOST, STANDARD_API | COLLAB_API) => {}
        _ => return None,
    }
    let mut params = BTreeMap::new();
    for (key, value) in url.query_pairs().filter(|(key, _)| {
        matches!(
            key.as_ref(),
            "authkey"
                | "authkey_ver"
                | "auth_appid"
                | "sign_type"
                | "game_biz"
                | "region"
                | "lang"
                | "timestamp"
        )
    }) {
        if params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return None;
        }
    }
    if params.get("auth_appid")?.as_str() != "webview_gacha"
        || params.get("game_biz")?.as_str() != "hkrpg_cn"
        || !matches!(params.get("region")?.as_str(), "prod_gf_cn" | "prod_qd_cn")
        || !valid_cookie_value(params.get("authkey")?)
    {
        return None;
    }
    let timestamp = params.get("timestamp")?.parse().ok()?;
    params
        .entry("authkey_ver".into())
        .or_insert_with(|| "1".into());
    params
        .entry("sign_type".into())
        .or_insert_with(|| "2".into());
    params
        .entry("lang".into())
        .or_insert_with(|| "zh-cn".into());
    Some(History { timestamp, params })
}

fn latest_link(blob: &str) -> Option<History> {
    blob.match_indices("https://")
        .filter_map(|(start, _)| {
            let candidate = &blob[start..];
            if !candidate.starts_with("https://webstatic.mihoyo.com/hkrpg/")
                && !candidate.starts_with("https://public-operation-hkrpg.mihoyo.com/common/")
            {
                return None;
            }
            let end = candidate
                .bytes()
                .take(4097)
                .take_while(|byte| {
                    byte.is_ascii_graphic()
                        && !matches!(byte, b'"' | b'\'' | b'<' | b'>' | b'{' | b'}' | b'\\')
                })
                .count();
            (end <= 4096)
                .then(|| parse_url(&candidate[..end]))
                .flatten()
        })
        .max_by_key(|history| history.timestamp)
}

fn read_file(path: &Path, limit: u64, tail: bool) -> Option<Vec<u8>> {
    // Windows File::open allows other processes to keep writing to these files.
    let mut file = File::open(path).ok()?;
    let length = file.metadata().ok()?.len();
    if length > limit {
        if !tail {
            return None;
        }
        file.seek(SeekFrom::Start(length - limit)).ok()?;
    }
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

fn data_directory(log: &str) -> Option<PathBuf> {
    for line in log.lines().rev() {
        let lower = line.to_ascii_lowercase();
        for (index, name) in lower.match_indices("starrail_data") {
            let bytes = line.as_bytes();
            if !index
                .checked_sub(1)
                .and_then(|at| bytes.get(at))
                .is_some_and(|byte| matches!(byte, b'/' | b'\\'))
            {
                continue;
            }
            let end = index + name.len();
            if !matches!(bytes.get(end), None | Some(b'/' | b'\\' | b'"')) {
                continue;
            }
            let start = bytes[..index].windows(3).rposition(|prefix| {
                prefix[0].is_ascii_alphabetic()
                    && prefix[1] == b':'
                    && matches!(prefix[2], b'/' | b'\\')
            });
            let Some(start) = start else {
                continue;
            };
            let directory = PathBuf::from(&line[start..end]);
            if directory.is_absolute() && directory.with_file_name("StarRail.exe").is_file() {
                return Some(directory);
            }
        }
    }
    None
}

pub(crate) fn find_history() -> Result<History, String> {
    let user = std::env::var_os("USERPROFILE").ok_or(OPEN_HISTORY)?;
    let root = PathBuf::from(user).join("AppData/LocalLow/miHoYo/崩坏：星穹铁道");
    let current = read_file(&root.join("Player.log"), 4 * 1024 * 1024, true);
    let current = current
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    if let Some(history) = latest_link(&current) {
        return Ok(history);
    }

    let previous = read_file(&root.join("Player-prev.log"), 4 * 1024 * 1024, true);
    let previous = previous
        .as_deref()
        .map(String::from_utf8_lossy)
        .unwrap_or_default();
    let mut latest = latest_link(&previous);
    if let Some(data) = data_directory(&current).or_else(|| data_directory(&previous)) {
        let caches = data.join("webCaches");
        let mut versions: Vec<_> = fs::read_dir(&caches)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.path().join("Cache/Cache_Data"))
            .filter_map(|path| Some((path.metadata().ok()?.modified().ok()?, path)))
            .collect();
        versions.sort_unstable_by(|a, b| b.0.cmp(&a.0));
        let dirs = versions
            .into_iter()
            .take(3)
            .map(|(_, dir)| dir)
            .chain(std::iter::once(caches.join("Cache/Cache_Data")));
        for dir in dirs {
            for name in ["data_2", "data_1", "data_3", "data_0"] {
                let Some(bytes) = read_file(&dir.join(name), 16 * 1024 * 1024, false) else {
                    continue;
                };
                if let Some(history) = latest_link(&String::from_utf8_lossy(&bytes)) {
                    if latest
                        .as_ref()
                        .is_none_or(|current| history.timestamp >= current.timestamp)
                    {
                        latest = Some(history);
                    }
                }
            }
        }
    }
    latest.ok_or_else(|| format!("未找到崩铁国服的跃迁历史链接。{OPEN_HISTORY}"))
}

async fn page(
    client: &Client,
    history: &History,
    endpoint: &str,
    pool: &str,
    index: u32,
    cursor: u64,
) -> Result<Vec<Value>, String> {
    let payload = response(
        client
            .get(format!("https://{HISTORY_HOST}{endpoint}"))
            .query(&history.params)
            .query(&[
                ("gacha_type", pool),
                ("size", "20"),
                ("page", &index.to_string()),
                ("end_id", &cursor.to_string()),
            ]),
    )
    .await?;
    match retcode(&payload) {
        Some(0) => {}
        Some(-100 | -101 | -108) => {
            return Err(format!("跃迁历史链接已失效或不完整。{OPEN_HISTORY}"));
        }
        Some(-110) => return Err("跃迁记录查询频繁，请稍后重试。".into()),
        Some(code) => return Err(format!("跃迁记录接口返回错误（{code}）。")),
        None => return Err("无法识别跃迁记录接口响应。".into()),
    }
    let mut payload = payload;
    match payload["data"]["list"].take() {
        Value::Array(rows) => Ok(rows),
        _ => Err("跃迁记录接口未返回有效列表。".into()),
    }
}

pub(crate) async fn fetch(
    client: &Client,
    history: &History,
    uid: &str,
) -> Result<Vec<Pull>, String> {
    let mut pulls = Vec::new();
    for (pool, label, endpoint) in POOLS {
        let mut cursor = 0;
        for index in 1.. {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let result = page(client, history, endpoint, pool, index, cursor)
                .await
                .map_err(|error| format!("{label}读取失败。{error}"))?;
            let records = gacha::online_page(Game::Starrail, uid, &result)?;
            let Some(last) = records.last() else {
                break;
            };
            let next = last
                .id
                .parse::<u64>()
                .ok()
                .filter(|next| *next > 0 && (cursor == 0 || *next < cursor))
                .ok_or("跃迁记录分页未向前推进，同步已停止。")?;
            cursor = next;
            let has_more = records.len() >= 20;
            pulls.extend(records);
            if !has_more {
                break;
            }
        }
    }
    Ok(pulls)
}

#[cfg(test)]
pub(crate) async fn current_uid(client: &Client) -> Result<String, String> {
    let history = find_history()?;
    let rows = page(client, &history, STANDARD_API, "11", 1, 0).await?;
    rows.first()
        .and_then(|row| crate::community::text(&row["uid"]))
        .ok_or_else(|| "测试需要角色活动跃迁历史中至少有一条记录。".into())
}
