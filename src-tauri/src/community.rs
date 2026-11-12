use crate::accounts::{Credentials, Game, Meter, Notes, Role, now};
use reqwest::{
    Client, RequestBuilder,
    header::{COOKIE, HeaderValue},
};
use serde_json::{Value, json};
use uuid::Uuid;

const ROLES: &str = "https://api-takumi.mihoyo.com/binding/api/getUserGameRolesByCookie";
const GENSHIN_NOTES: &str =
    "https://api-takumi-record.mihoyo.com/game_record/app/genshin/api/dailyNote";
const GENSHIN_WIDGET: &str =
    "https://api-takumi-record.mihoyo.com/game_record/app/genshin/aapi/widget/v2";
const STARRAIL_NOTES: &str = "https://api-takumi-record.mihoyo.com/game_record/app/hkrpg/api/note";
const STARRAIL_WIDGET: &str =
    "https://api-takumi-record.mihoyo.com/game_record/app/hkrpg/aapi/widget";
const LTOKEN: &str = "https://passport-api.mihoyo.com/account/auth/api/getLTokenBySToken";
const DEVICE_FP: &str = "https://public-data-api.mihoyo.com/device-fp/api/getFp";

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

pub(crate) fn valid_cookie_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 8_192
        && value
            .bytes()
            .all(|ch| ch.is_ascii_graphic() && !matches!(ch, b';' | b',' | b'"'))
}

fn cookie<'a>(credentials: &'a Credentials, name: &str) -> Option<&'a str> {
    credentials.secret.split(';').find_map(|entry| {
        let (key, value) = entry.trim().split_once('=')?;
        (key == name && valid_cookie_value(value)).then_some(value)
    })
}

fn signed(salt: &str, body: &str, query: &str) -> String {
    let timestamp = now();
    let random = *Uuid::new_v4().as_bytes();
    let nonce = 100_001 + u32::from(u16::from_be_bytes([random[0], random[1]]));
    let digest = md5::compute(format!(
        "salt={salt}&t={timestamp}&r={nonce}&b={body}&q={query}"
    ));
    format!("{timestamp},{nonce},{digest:x}")
}

pub(crate) fn passport_ds(body: &str, query: &str) -> String {
    signed("xV8v4Qu54lUKrEYFZkJhB8cuOh9Asafs", body, query)
}

fn authorized(
    request: RequestBuilder,
    credentials: &Credentials,
    signed_query: &str,
) -> Result<RequestBuilder, String> {
    // CN request signature and routes are documented by the MIT-licensed genshin.py.
    let mut request = request
        .header(COOKIE, sensitive(&credentials.secret)?)
        .header("origin", "https://webstatic.mihoyo.com")
        .header("referer", "https://webstatic.mihoyo.com/")
        .header("x-rpc-app_version", "2.99.1")
        .header("x-rpc-client_type", "5")
        .header("ds", passport_ds("", signed_query));
    if !credentials.device_id.is_empty() {
        request = request.header("x-rpc-device_id", sensitive(&credentials.device_id)?);
    }
    if !credentials.device_extra.is_empty() {
        request = request.header("x-rpc-device_fp", sensitive(&credentials.device_extra)?);
    }
    Ok(request)
}

pub(crate) async fn response(request: RequestBuilder) -> Result<Value, String> {
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
    response
        .json()
        .await
        .map_err(|_| "无法识别米游社响应。".into())
}

pub(crate) fn retcode(payload: &Value) -> Option<i64> {
    payload["retcode"]
        .as_i64()
        .or_else(|| payload["retcode"].as_str()?.parse().ok())
}

pub(crate) fn data(mut payload: Value) -> Result<Value, String> {
    let code = retcode(&payload).ok_or("无法识别米游社响应状态。")?;
    if code != 0 {
        return Err(match code {
            -111 | -100 | -101 | 10001 => "米游社登录已失效，请重新扫码。".into(),
            1034 | 5003 | 10035 | 10041 | -3101 | -3102 => {
                format!("米游社要求安全验证（{code}），请在官方 App 检查登录和便笺。")
            }
            -10001 => "米游社未接受当前便笺请求（-10001），请检查登录状态。".into(),
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

async fn read_data(request: RequestBuilder) -> Result<Value, String> {
    data(response(request).await?)
}

pub(crate) fn text(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(str::to_owned)
        .or_else(|| value.as_u64().map(|value| value.to_string()))
}

pub(crate) fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| value.as_str()?.parse().ok())
}

/// Complete the optional parts of a Passport App login session. A working
/// SToken remains useful for widgets when a token or fingerprint is unavailable.
pub(crate) async fn prepare_session(client: &Client, credentials: &mut Credentials) {
    if cookie(credentials, "ltoken").is_none() && cookie(credentials, "ltoken_v2").is_none() {
        if let Ok(header) = sensitive(&credentials.secret) {
            let request = client
                .get(LTOKEN)
                .header(COOKIE, header)
                .header("x-rpc-client_type", "1")
                .header("x-rpc-game_biz", "bbs_cn")
                .header("x-rpc-app_version", "2.63.1")
                .header("x-rpc-device_id", &credentials.device_id);
            if let Ok(data) = read_data(request).await {
                let token = data["ltoken"]
                    .as_str()
                    .or_else(|| data["ltoken_v2"].as_str())
                    .filter(|value| valid_cookie_value(value));
                if let (Some(token), Some(uid), Some(mid)) = (
                    token,
                    cookie(credentials, "stuid"),
                    cookie(credentials, "mid"),
                ) {
                    let supplement = if token.starts_with("v2_") {
                        format!("; ltoken_v2={token}; ltuid_v2={uid}; ltmid_v2={mid}")
                    } else {
                        format!("; ltoken={token}; ltuid={uid}")
                    };
                    credentials.secret.push_str(&supplement);
                }
            }
        }
    }
    if credentials.device_extra.is_empty() {
        let seed = Uuid::new_v4().simple().to_string();
        let seed_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let request = client.post(DEVICE_FP).json(&json!({
            "seed_id": &seed[..16],
            "device_id": credentials.device_id.to_lowercase(),
            "platform": "5",
            "seed_time": seed_time.to_string(),
            "device_fp": &seed[16..29],
            "app_name": "account_cn",
            "ext_fields": json!({
                "userAgent": "GameDailyHub/0.1",
                "browserLanguage": "zh-CN",
                "browserPlat": "Win32",
                "isTouchSupported": false,
                "maxTouchPoints": 0,
            }).to_string(),
        }));
        if let Ok(data) = read_data(request).await {
            if let Some(fp) = data["device_fp"]
                .as_str()
                .filter(|value| value.len() <= 256 && valid_cookie_value(value))
            {
                credentials.device_extra = fp.to_owned();
            }
        }
    }
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
    single_role: bool,
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
    // Widgets use the App session issued by QR login and select a role server-side.
    // When several roles exist, the role-specific endpoints use explicit UID and server.
    let use_widget = single_role
        && cookie(credentials, "stoken").is_some_and(|value| value.starts_with("v2_"))
        && cookie(credentials, "mid").is_some();
    let data = if use_widget && role.game == Game::Genshin {
        read_data(authorized(client.get(GENSHIN_WIDGET), credentials, "")?).await?
    } else if use_widget {
        let request = client
            .get(STARRAIL_WIDGET)
            .header(COOKIE, sensitive(&credentials.secret)?)
            .header("referer", "https://app.mihoyo.com")
            .header("accept-language", "zh-CN")
            .header(
                "user-agent",
                "WidgetExtension/231 CFNetwork/1390 Darwin/22.0.0",
            )
            .header("x-rpc-app_version", "2.63.1")
            .header("x-rpc-client_type", "2")
            .header("x-rpc-channel", "appstore")
            .header("ds", signed("t0qEgfub6cvueAPgR5m9aQWWVciEer7v", "", ""));
        read_data(request).await?
    } else {
        let (tool_version, page) = if role.game == Game::Genshin {
            ("v4.2.2-ys", "v4.2.2-ys_#/ys/daily")
        } else {
            ("v4.51.1", "v4.51.1_#/rpg")
        };
        let request = client
            .get(url)
            .query(&query)
            .header("x-rpc-tool_version", tool_version)
            .header("x-rpc-page", page);
        read_data(authorized(request, credentials, &signed_query)?).await?
    };
    if text(&data["role_id"])
        .or_else(|| text(&data["game_uid"]))
        .filter(|uid| !uid.is_empty())
        .is_some_and(|uid| uid != role.uid)
    {
        return Err("米游社返回的便笺角色与账号不一致。".into());
    }
    let (stamina, activity, recovery) = if role.game == Game::Genshin {
        (
            meter(&data["current_resin"], &data["max_resin"]),
            meter(
                &data["daily_task"]["finished_num"],
                &data["daily_task"]["total_num"],
            )
            .or_else(|| meter(&data["finished_task_num"], &data["total_task_num"])),
            number(&data["resin_recovery_time"]),
        )
    } else {
        (
            meter(&data["current_stamina"], &data["max_stamina"]),
            meter(&data["current_train_score"], &data["max_train_score"])
                .or_else(|| meter(&data["current_training_score"], &data["max_training_score"])),
            number(&data["stamina_recover_time"]),
        )
    };
    let stamina = stamina.ok_or("角色资料已更新，米游社未返回可识别的体力字段。")?;
    Ok(Notes {
        stamina: Some(stamina),
        activity,
        full_at: recovery
            .filter(|seconds| *seconds <= 604_800)
            .map(|seconds| now() + seconds),
    })
}
