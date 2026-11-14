use crate::{
    accounts::{AccountStore, Credentials, Game, Role, now},
    community::{self, number, text},
};
use reqwest::{Client, RequestBuilder, header::COOKIE};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use tauri::State;
use uuid::Uuid;

const GENSHIN_LIST: &str =
    "https://api-takumi.mihoyo.com/event/e20200928calculate/v1/sync/avatar/list";
const STARRAIL_LIST: &str = "https://api-takumi.mihoyo.com/event/rpgcalc/avatar/list";
const STARRAIL_DETAIL: &str = "https://api-takumi.mihoyo.com/event/rpgcalc/avatar/detail";
const PAGE_SIZE: usize = 100;
const INVALID_LIST: &str = "无法识别米游社返回的角色列表。";
const INVALID_CHARACTER: &str = "无法识别米游社返回的角色资料。";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CharacterRoster {
    fetched_at: u64,
    characters: Vec<Character>,
}

#[derive(Serialize)]
struct Character {
    key: String,
    id: u32,
    name: String,
    level: Option<u64>,
    rank: Option<u64>,
    details: Option<CharacterDetails>,
}

#[derive(Serialize)]
pub struct CharacterDetails {
    weapon: Option<LeveledItem>,
    artifacts: Vec<LeveledItem>,
    skills: Vec<LeveledItem>,
}

#[derive(Serialize)]
struct LeveledItem {
    name: String,
    level: Option<u64>,
}

#[tauri::command]
pub async fn get_characters(
    id: Uuid,
    store: State<'_, AccountStore>,
) -> Result<CharacterRoster, String> {
    let (role, credentials) = store.linked_session(id).await?;
    fetch_roster(&store.client, &credentials, &role).await
}

#[tauri::command]
pub async fn get_starrail_character(
    id: Uuid,
    character_id: u32,
    store: State<'_, AccountStore>,
) -> Result<CharacterDetails, String> {
    let (role, credentials) = store.linked_session(id).await?;
    fetch_starrail_details(&store.client, &credentials, &role, character_id).await
}

fn cultivation_request(
    request: RequestBuilder,
    credentials: &Credentials,
) -> Result<RequestBuilder, String> {
    let mut request = request
        .header(COOKIE, community::sensitive(&credentials.secret)?)
        .header("origin", "https://act.mihoyo.com")
        .header("referer", "https://act.mihoyo.com/")
        .header("x-rpc-platform", "4")
        .header("x-rpc-lang", "zh-cn");
    if !credentials.device_id.is_empty() {
        request = request.header(
            "x-rpc-device_id",
            community::sensitive(&credentials.device_id)?,
        );
    }
    if !credentials.device_extra.is_empty() {
        request = request.header(
            "x-rpc-device_fp",
            community::sensitive(&credentials.device_extra)?,
        );
    }
    Ok(request)
}

fn starrail_request(
    client: &Client,
    url: &str,
    credentials: &Credentials,
    role: &Role,
) -> Result<RequestBuilder, String> {
    let request = client.get(url).query(&[
        ("game", "hkrpg"),
        ("uid", role.uid.as_str()),
        ("region", role.server.as_str()),
        ("lang", "zh-cn"),
        ("tab_from", "TabOwned"),
    ]);
    Ok(cultivation_request(request, credentials)?
        .header("x-rpc-page", "v4.4.4__#/tools/calculation")
        .header("x-rpc-view_source", "1"))
}

async fn fetch_roster(
    client: &Client,
    credentials: &Credentials,
    role: &Role,
) -> Result<CharacterRoster, String> {
    let mut characters = Vec::new();
    let mut seen = HashSet::new();
    for page in 1..=20 {
        let request = match role.game {
            Game::Genshin => {
                let request = client.post(GENSHIN_LIST).json(&json!({
                    "element_attr_ids": [],
                    "weapon_cat_ids": [],
                    "page": page,
                    "size": PAGE_SIZE,
                    "uid": role.uid,
                    "region": role.server,
                    "lang": "zh-cn"
                }));
                cultivation_request(request, credentials)?.header("x-rpc-page", "__#")
            }
            Game::Starrail => starrail_request(client, STARRAIL_LIST, credentials, role)?
                .query(&[("page", page), ("size", PAGE_SIZE)]),
            _ => return Err("该游戏暂未接入角色查询。".into()),
        };
        let data = community::read_data(request).await?;
        if role.game == Game::Starrail {
            require_starrail_login(&data)?;
        }
        let rows = data["list"].as_array().ok_or(INVALID_LIST)?;
        for row in rows {
            let character = parse_character(role.game, row)?;
            if !seen.insert(character.key.clone()) {
                return Err("米游社返回了重复角色，请稍后重试。".into());
            }
            characters.push(character);
        }

        let complete = match number(&data["total"]) {
            Some(total) => {
                if characters.len() as u64 > total
                    || (rows.is_empty() && (characters.len() as u64) < total)
                {
                    return Err(INVALID_LIST.into());
                }
                characters.len() as u64 == total
            }
            None => rows.len() < PAGE_SIZE,
        };
        if complete {
            return Ok(CharacterRoster {
                fetched_at: now(),
                characters,
            });
        }
    }
    Err("角色列表未能读取完整，请稍后重试。".into())
}

fn require_starrail_login(data: &Value) -> Result<(), String> {
    if data["is_login"].as_bool() == Some(true) {
        Ok(())
    } else {
        Err("崩铁养成计算器未识别当前登录，请重新连接米游社后刷新。".into())
    }
}

fn nonempty_text(value: &Value) -> Option<String> {
    text(value).filter(|value| !value.trim().is_empty())
}

fn parse_character(game: Game, row: &Value) -> Result<Character, String> {
    let (id, name, level, rank, details) = if game == Game::Genshin {
        (
            &row["id"],
            &row["name"],
            number(&row["level_current"]).or_else(|| number(&row["avatar_level"])),
            &row["constellation_num"],
            Some(genshin_details(row)),
        )
    } else {
        (
            &row["item_id"],
            &row["item_name"],
            number(&row["cur_level"]),
            &row["rank_level"],
            None,
        )
    };
    let id = number(id)
        .and_then(|id| u32::try_from(id).ok())
        .filter(|&id| id != 0)
        .ok_or(INVALID_CHARACTER)?;
    let key =
        if let Some(element) = number(&row["element_attr_id"]).filter(|_| game == Game::Genshin) {
            format!("{id}:{element}")
        } else {
            id.to_string()
        };
    Ok(Character {
        key,
        id,
        name: nonempty_text(name).ok_or(INVALID_CHARACTER)?,
        level,
        rank: number(rank),
        details,
    })
}

fn leveled_item(name: &Value, level: Option<u64>) -> Option<LeveledItem> {
    Some(LeveledItem {
        name: nonempty_text(name)?,
        level,
    })
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

fn genshin_details(row: &Value) -> CharacterDetails {
    let equipped = &row["weapon"];
    let weapon = leveled_item(
        &equipped["name"],
        number(&equipped["level_current"]).or_else(|| number(&equipped["weapon_level"])),
    );
    let artifacts = array(&row["reliquary_list"])
        .iter()
        .filter_map(|item| {
            leveled_item(
                &item["name"],
                number(&item["level_current"]).or_else(|| number(&item["reliquary_level"])),
            )
        })
        .collect();
    let skills = array(&row["skill_list"])
        .iter()
        .filter_map(|item| leveled_item(&item["name"], number(&item["level_current"])))
        .collect();
    CharacterDetails {
        weapon,
        artifacts,
        skills,
    }
}

async fn fetch_starrail_details(
    client: &Client,
    credentials: &Credentials,
    role: &Role,
    character_id: u32,
) -> Result<CharacterDetails, String> {
    if role.game != Game::Starrail {
        return Err("仅崩铁角色需要单独查询行迹。".into());
    }
    if character_id == 0 {
        return Err(INVALID_CHARACTER.into());
    }
    let request = starrail_request(client, STARRAIL_DETAIL, credentials, role)?
        .query(&[("item_id", character_id), ("change_target_level", 0)]);
    parse_starrail_details(&community::read_data(request).await?, character_id)
}

fn parse_starrail_details(data: &Value, character_id: u32) -> Result<CharacterDetails, String> {
    require_starrail_login(data)?;
    if number(&data["avatar"]["item_id"]) != Some(u64::from(character_id)) {
        return Err("米游社返回的详情与所选角色不一致。".into());
    }
    let equipped = &data["equipment"];
    let weapon = leveled_item(&equipped["item_name"], number(&equipped["cur_level"]));

    // The suffix identifies the trace even when the server omits another point.
    let mut skills: Vec<_> = array(&data["skills"])
        .iter()
        .filter_map(|skill| {
            let name = match (
                number(&skill["point_type"])?,
                number(&skill["point_id"])? % 1000,
            ) {
                (2, 1) => "普攻",
                (2, 2) => "战技",
                (2, 3) => "终结技",
                (2, 4) => "天赋",
                (4, _) => "欢愉技",
                _ => "额外行迹",
            };
            Some(LeveledItem {
                name: name.into(),
                level: Some(number(&skill["cur_level"])?),
            })
        })
        .collect();
    skills.extend(array(&data["skills_servant"]).iter().filter_map(|skill| {
        let name = match number(&skill["point_id"])? % 1000 {
            19 => "忆灵技",
            20 => "忆灵天赋",
            _ => "忆灵行迹",
        };
        Some(LeveledItem {
            name: name.into(),
            level: Some(number(&skill["cur_level"])?),
        })
    }));
    Ok(CharacterDetails {
        weapon,
        artifacts: Vec::new(),
        skills,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starrail_details_use_the_returned_id_and_allow_a_missing_acquisition_time() {
        let mut data = json!({
            "is_login": true,
            "avatar": {"item_id": "1001", "first_meet_ts": "0"},
            "equipment": null,
            "skills": [
                {"point_id": "1001001", "point_type": 2, "cur_level": "1"},
                {"point_id": "1001003", "point_type": 2, "cur_level": 3},
            ]
        });
        let detail = parse_starrail_details(&data, 1001).unwrap();
        assert!(detail.weapon.is_none());
        assert_eq!(detail.skills[1].name, "终结技");
        assert_eq!(detail.skills[1].level, Some(3));
        assert!(parse_starrail_details(&data, 1002).is_err());
        data["is_login"] = json!(false);
        assert!(parse_starrail_details(&data, 1001).is_err());
    }
}
