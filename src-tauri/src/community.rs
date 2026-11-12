use crate::accounts::{Credentials, Game, Meter, Notes, Role, now};
use reqwest::{
    Client, RequestBuilder,
    header::{COOKIE, HeaderValue},
};
use serde_json::Value;
use uuid::Uuid;

const ROLES: &str = "https://api-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie";
const GENSHIN_NOTES: &str =
    "https://api-takumi-record.mihoyo.com/game_record/app/genshin/api/dailyNote";
const STARRAIL_NOTES: &str = "https://api-takumi-record.mihoyo.com/game_record/app/hkrpg/api/note";

pub fn validate_credentials(credentials: &Credentials) -> Result<(), String> {
    if credentials.secret.is_empty()
        || credentials.secret.len() > 16_384
        || HeaderValue::from_str(&credentials.secret).is_err()
    {
        return Err("请输入有效的单行米游社 Cookie。".into());
    }
    for value in [&credentials.device_id, &credentials.device_extra] {
        if value.len() > 256 || HeaderValue::from_str(value).is_err() {
            return Err("设备信息格式无效。".into());
        }
    }
    Ok(())
}

fn sensitive(value: &str) -> Result<HeaderValue, String> {
    let mut header = HeaderValue::from_str(value).map_err(|_| "凭据格式无效。")?;
    header.set_sensitive(true);
    Ok(header)
}

fn authorized(
    request: RequestBuilder,
    credentials: &Credentials,
    signed_query: &str,
) -> Result<RequestBuilder, String> {
    // CN request signature and routes are documented by the MIT-licensed genshin.py.
    let timestamp = now();
    let random = *Uuid::new_v4().as_bytes();
    let nonce = 100_001 + u32::from(u16::from_be_bytes([random[0], random[1]]));
    let digest = md5::compute(format!(
        "salt=xV8v4Qu54lUKrEYFZkJhB8cuOh9Asafs&t={timestamp}&r={nonce}&b=&q={signed_query}"
    ));
    let mut request = request
        .header(COOKIE, sensitive(&credentials.secret)?)
        .header("referer", "https://www.miyoushe.com/")
        .header("x-rpc-app_version", "2.11.1")
        .header("x-rpc-client_type", "5")
        .header("ds", format!("{timestamp},{nonce},{digest:x}"));
    if !credentials.device_id.is_empty() {
        request = request.header("x-rpc-device_id", sensitive(&credentials.device_id)?);
    }
    if !credentials.device_extra.is_empty() {
        request = request.header("x-rpc-device_fp", sensitive(&credentials.device_extra)?);
    }
    Ok(request)
}

async fn read_data(request: RequestBuilder) -> Result<Value, String> {
    let response = request.send().await.map_err(|error| {
        if error.is_timeout() {
            "米游社查询超时，请稍后刷新。".to_string()
        } else {
            "无法连接米游社，请检查网络。".to_string()
        }
    })?;
    if !response.status().is_success() {
        return Err(match response.status().as_u16() {
            401 => "米游社登录已失效，请更新凭据。".into(),
            403 => "米游社限制了查询，请在官方 App 检查登录与验证状态。".into(),
            429 => "米游社查询频繁，请稍后刷新。".into(),
            status => format!("米游社查询失败（HTTP {status}）。"),
        });
    }
    let mut payload: Value = response.json().await.map_err(|_| "无法识别米游社响应。")?;
    let code = payload["retcode"]
        .as_i64()
        .or_else(|| payload["retcode"].as_str()?.parse().ok())
        .ok_or("无法识别米游社响应状态。")?;
    if code != 0 {
        return Err(match code {
            -100 | -101 => "米游社登录已失效，请更新凭据。".into(),
            1034 | 10035 | -3101 | -10001 => {
                format!("米游社限制了查询，请检查官方 App 和设备信息（{code}）。")
            }
            10101 | 10102 => "请在米游社检查角色及实时便笺的展示设置。".into(),
            _ => format!("米游社返回查询失败（{code}），请检查登录状态与展示设置。"),
        });
    }
    let data = payload["data"].take();
    if data.is_null() {
        return Err("米游社未返回数据，请检查角色绑定和展示设置。".into());
    }
    Ok(data)
}

fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|value| value.to_string()))
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

fn meter(current: &Value, maximum: &Value) -> Option<Meter> {
    let maximum = u32::try_from(number(maximum)?).ok()?;
    if maximum == 0 {
        return None;
    }
    Some(Meter {
        current: number(current)?.try_into().ok()?,
        maximum,
    })
}

fn role(value: &Value) -> Option<Role> {
    let game = match value["game_biz"].as_str()? {
        "hk4e_cn" => Game::Genshin,
        "hkrpg_cn" => Game::Starrail,
        _ => return None,
    };
    let uid = text(&value["game_uid"]).filter(|value| !value.is_empty())?;
    let server = text(&value["region"]).filter(|value| !value.is_empty())?;
    Some(Role {
        game,
        name: text(&value["nickname"])
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| uid.clone()),
        server_name: text(&value["region_name"])
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| server.clone()),
        level: number(&value["level"]).and_then(|level| level.try_into().ok()),
        uid,
        server,
    })
}

pub async fn roles(client: &Client, credentials: &Credentials) -> Result<Vec<Role>, String> {
    let data = read_data(authorized(client.get(ROLES), credentials, "")?).await?;
    let entries = data["list"]
        .as_array()
        .ok_or("无法识别米游社返回的角色列表。")?;
    Ok(entries.iter().filter_map(role).collect())
}

pub async fn notes(
    client: &Client,
    credentials: &Credentials,
    role: &Role,
) -> Result<Notes, String> {
    let url = match role.game {
        Game::Genshin => GENSHIN_NOTES,
        Game::Starrail => STARRAIL_NOTES,
        _ => return Err("该角色暂未接入米游社便笺。".into()),
    };
    let query = [
        ("role_id", role.uid.as_str()),
        ("server", role.server.as_str()),
    ];
    let signed_query = format!("role_id={}&server={}", role.uid, role.server);
    let data = read_data(authorized(
        client.get(url).query(&query),
        credentials,
        &signed_query,
    )?)
    .await?;
    let (stamina, activity, recovery) = if role.game == Game::Genshin {
        (
            meter(&data["current_resin"], &data["max_resin"]),
            meter(&data["finished_task_num"], &data["total_task_num"]),
            number(&data["resin_recovery_time"]),
        )
    } else {
        (
            meter(&data["current_stamina"], &data["max_stamina"]),
            meter(&data["current_train_score"], &data["max_train_score"]),
            number(&data["stamina_recover_time"]),
        )
    };
    if stamina.is_none() && activity.is_none() {
        return Err("角色资料已更新，米游社未返回可识别的便笺字段。".into());
    }
    Ok(Notes {
        stamina,
        activity,
        full_at: recovery
            .filter(|seconds| *seconds <= 604_800)
            .map(|seconds| now() + seconds),
    })
}
