use crate::{
    accounts::{Credentials, Game, Role, now},
    community::{self, number},
};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum Mode {
    Theater,
    StygianOnslaught,
    AnomalyArbitration,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BattleRecords {
    fetched_at: u64,
    modes: Vec<ModeRecord>,
}

#[derive(Serialize)]
struct ModeRecord {
    mode: Mode,
    #[serde(flatten)]
    outcome: Outcome,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum Outcome {
    Records { seasons: Vec<Season> },
    Locked,
    Error { message: String },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Season {
    name: Option<String>,
    starts_on: Option<ScheduleDate>,
    ends_on: Option<ScheduleDate>,
    groups: Vec<Progress>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum ScheduleDate {
    Timestamp(u64),
    CalendarDate(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    name: Option<&'static str>,
    has_record: Option<bool>,
    metrics: Vec<Metric>,
}

#[derive(Serialize)]
struct Metric {
    label: &'static str,
    value: String,
}

pub async fn fetch(
    client: &Client,
    credentials: &Credentials,
    role: &Role,
) -> Result<BattleRecords, String> {
    let modes: &[Mode] = match role.game {
        Game::Genshin => &[Mode::Theater, Mode::StygianOnslaught],
        Game::Starrail => &[Mode::AnomalyArbitration],
        _ => return Err("该游戏暂未接入战绩查询。".into()),
    };
    let mut records = Vec::with_capacity(modes.len());
    for &mode in modes {
        let (path, params): (_, &[(&str, &str)]) = match mode {
            Mode::Theater => (
                "genshin/api/role_combat",
                &[("active", "1"), ("need_detail", "true")],
            ),
            Mode::StygianOnslaught => ("genshin/api/hard_challenge", &[("need_detail", "true")]),
            Mode::AnomalyArbitration => ("hkrpg/api/challenge_peak", &[("schedule_type", "1")]),
        };
        let result = community::battle_data(client, credentials, role, path, params)
            .await
            .and_then(|data| parse(mode, &data, role));
        records.push(ModeRecord {
            mode,
            outcome: result.unwrap_or_else(|message| Outcome::Error { message }),
        });
    }
    Ok(BattleRecords {
        fetched_at: now(),
        modes: records,
    })
}

fn parse(mode: Mode, data: &Value, role: &Role) -> Result<Outcome, String> {
    if data["is_unlock"].as_bool() == Some(false) {
        return Ok(Outcome::Locked);
    }
    if community::text(&data["role"]["role_id"])
        .filter(|uid| !uid.is_empty())
        .is_some_and(|uid| uid != role.uid)
    {
        return Err("米游社返回的战绩与当前角色不一致。".into());
    }
    let (field, summarize): (&str, fn(&Value) -> Season) = match mode {
        Mode::Theater => ("data", theater),
        Mode::StygianOnslaught => ("data", stygian),
        Mode::AnomalyArbitration => ("challenge_peak_records", anomaly),
    };
    let periods = data[field].as_array().ok_or("无法识别米游社的战绩期次。")?;
    let seasons = periods
        .iter()
        .filter(|period| period["schedule"]["is_valid"].as_bool() != Some(false))
        .map(summarize)
        .collect();
    Ok(Outcome::Records { seasons })
}

fn nonempty_text(value: &Value) -> Option<String> {
    let value = value.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn schedule_date(value: &Value) -> Option<ScheduleDate> {
    if let Some(timestamp) = number(value) {
        return (timestamp > 0 && timestamp <= 253_402_300_799)
            .then_some(ScheduleDate::Timestamp(timestamp));
    }
    let year = number(&value["year"])?;
    let month = number(&value["month"])?;
    let day = number(&value["day"])?;
    (year > 0 && year <= 9999 && (1..=12).contains(&month) && (1..=31).contains(&day))
        .then(|| ScheduleDate::CalendarDate(format!("{year:04}-{month:02}-{day:02}")))
}

fn count(data: &Value, field: &str, label: &'static str, suffix: &str) -> Option<Metric> {
    Some(Metric {
        label,
        value: format!("{}{suffix}", number(&data[field])?),
    })
}

fn theater(period: &Value) -> Season {
    let schedule = &period["schedule"];
    let stat = &period["stat"];
    let difficulty = number(&stat["difficulty_id"])
        .filter(|id| *id > 0)
        .map(|id| Metric {
            label: "难度",
            value: match id {
                1 => "轻简".into(),
                2 => "普通".into(),
                3 => "困难".into(),
                4 => "卓越".into(),
                5 => "月谕".into(),
                _ => id.to_string(),
            },
        });
    Season {
        name: None,
        starts_on: schedule_date(&schedule["start_time"])
            .or_else(|| schedule_date(&schedule["start_date_time"])),
        ends_on: schedule_date(&schedule["end_time"])
            .or_else(|| schedule_date(&schedule["end_date_time"])),
        groups: vec![Progress {
            name: None,
            has_record: period["has_data"].as_bool(),
            metrics: [
                difficulty,
                count(stat, "max_round_id", "最高幕数", " 幕"),
                count(stat, "medal_num", "明星挑战", " 星"),
                count(stat, "tarot_finished_cnt", "月谕挑战", " 场"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }],
    }
}

fn stygian(period: &Value) -> Season {
    let schedule = &period["schedule"];
    let groups = [("single", "单人"), ("mp", "联机")]
        .into_iter()
        .filter_map(|(field, label)| {
            let run = &period[field];
            let has_record = run["has_data"].as_bool()?;
            let best = &run["best"];
            Some(Progress {
                name: Some(label),
                has_record: Some(has_record),
                metrics: [
                    count(best, "difficulty", "最高难度", ""),
                    count(best, "second", "用时", " 秒"),
                ]
                .into_iter()
                .flatten()
                .collect(),
            })
        })
        .collect();
    Season {
        name: nonempty_text(&schedule["name"]),
        starts_on: schedule_date(&schedule["start_time"]),
        ends_on: schedule_date(&schedule["end_time"]),
        groups,
    }
}

fn anomaly(period: &Value) -> Season {
    let season = &period["group"];
    Season {
        name: nonempty_text(&season["name_mi18n"]),
        starts_on: schedule_date(&season["begin_time"]),
        ends_on: schedule_date(&season["end_time"]),
        groups: vec![Progress {
            name: None,
            has_record: period["has_challenge_record"].as_bool(),
            metrics: [
                count(period, "mob_stars", "骑士关卡", " 星"),
                count(period, "boss_stars", "王棋关卡", " 星"),
                count(period, "battle_num", "挑战次数", " 次"),
            ]
            .into_iter()
            .flatten()
            .collect(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn recorded_zero_stars_and_empty_periods_are_distinct() {
        let record = anomaly(&json!({
            "has_challenge_record": true,
            "mob_stars": 3,
            "boss_stars": 0,
            "battle_num": "2",
            "group": { "begin_time": { "year": 2026, "month": 11, "day": 1 } },
        }));
        assert!(
            matches!(record.starts_on, Some(ScheduleDate::CalendarDate(date)) if date == "2026-11-01")
        );
        assert_eq!(record.groups[0].has_record, Some(true));
        assert_eq!(record.groups[0].metrics[1].value, "0 星");

        let empty = theater(&json!({
            "has_data": false,
            "schedule": { "start_time": "1793462400" },
            "stat": { "medal_num": 0 },
        }));
        assert_eq!(empty.groups[0].has_record, Some(false));
        assert!(matches!(
            empty.starts_on,
            Some(ScheduleDate::Timestamp(1793462400))
        ));
    }
}
