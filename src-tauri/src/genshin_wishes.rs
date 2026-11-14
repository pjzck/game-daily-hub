use crate::{
    accounts::{Credentials, Role, now},
    community::{cookie, read_data, response, retcode, sensitive, text, valid_cookie_value},
    gacha::{self, Pull},
};
use percent_encoding::percent_decode_str;
use reqwest::{Client, header::COOKIE};
use serde_json::json;
use std::time::Duration;
use uuid::Uuid;

const AUTH_URL: &str = "https://api-takumi.miyoushe.com/binding/api/genAuthKey";
const HISTORY_URL: &str = "https://public-operation-hk4e.mihoyo.com/gacha_info/api/getGachaLog";
const POOLS: [(&str, &str); 5] = [
    ("301", "角色活动祈愿"),
    ("302", "武器活动祈愿"),
    ("500", "集录祈愿"),
    ("200", "常驻祈愿"),
    ("100", "新手祈愿"),
];

struct WishPermission {
    key: String,
    version: String,
    sign_type: String,
}

async fn authorize(
    client: &Client,
    role: &Role,
    credentials: &Credentials,
) -> Result<WishPermission, String> {
    let stoken = cookie(credentials, "stoken").or_else(|| cookie(credentials, "stoken_v2"));
    let mid = cookie(credentials, "mid").or_else(|| cookie(credentials, "account_mid_v2"));
    let (Some(stoken), Some(mid)) = (stoken, mid) else {
        return Err("该账号缺少祈愿同步所需的会话，请更新米游社扫码连接。".into());
    };
    // The CN binding API uses an LK2 signature for a role-scoped wish history key.
    let timestamp = now();
    let nonce = Uuid::new_v4().simple().to_string();
    let nonce = &nonce[..6];
    let digest = md5::compute(format!(
        "salt=d9200c846b10886e8c874fc33c8f308b&t={timestamp}&r={nonce}"
    ));
    let mut request = client.post(AUTH_URL)
        .header(COOKIE, sensitive(&format!("stoken={stoken}; mid={mid}"))?)
        .header("referer", "https://webstatic.mihoyo.com/")
        .header("x-rpc-app_version", "2.109.0")
        .header("x-rpc-client_type", "5")
        .header("x-requested-with", "com.mihoyo.hyperion")
        .header("ds", format!("{timestamp},{nonce},{digest:x}"))
        .json(&json!({ "game_biz": "hk4e_cn", "game_uid": role.uid, "region": role.server, "auth_appid": "webview_gacha" }));
    if !credentials.device_id.is_empty() {
        request = request.header("x-rpc-device_id", sensitive(&credentials.device_id)?);
    }
    if !credentials.device_extra.is_empty() {
        request = request.header("x-rpc-device_fp", sensitive(&credentials.device_extra)?);
    }
    let data = read_data(request).await?;
    let authkey = data["authkey"].as_str().ok_or("米游社未返回祈愿授权。")?;
    let key = percent_decode_str(authkey)
        .decode_utf8()
        .map_err(|_| "无法识别祈愿授权。")?;
    if !valid_cookie_value(&key) {
        return Err("无法识别祈愿授权。".into());
    }
    Ok(WishPermission {
        key: key.into_owned(),
        version: text(&data["authkey_ver"]).unwrap_or_else(|| "1".into()),
        sign_type: text(&data["sign_type"]).unwrap_or_else(|| "2".into()),
    })
}

async fn history_page(
    client: &Client,
    role: &Role,
    permission: &WishPermission,
    pool: &str,
    cursor: &str,
    page: u32,
) -> Result<Vec<Pull>, String> {
    let payload = response(client.get(HISTORY_URL).query(&[
        ("authkey", permission.key.as_str()),
        ("authkey_ver", permission.version.as_str()),
        ("sign_type", permission.sign_type.as_str()),
        ("auth_appid", "webview_gacha"),
        ("game_biz", "hk4e_cn"),
        ("region", role.server.as_str()),
        ("lang", "zh-cn"),
        ("gacha_type", pool),
        ("size", "20"),
        ("page", &page.to_string()),
        ("end_id", cursor),
    ]))
    .await?;
    match retcode(&payload) {
        Some(0) => {}
        Some(-100 | -101) => return Err("祈愿授权已失效，请重新同步。".into()),
        Some(-110) => return Err("祈愿查询频繁，请稍后重试。".into()),
        Some(code) => return Err(format!("祈愿接口返回错误（{code}）。")),
        None => return Err("无法识别祈愿接口响应。".into()),
    }
    let rows = payload["data"]["list"]
        .as_array()
        .ok_or("祈愿接口未返回有效列表。")?;
    gacha::wish_page(role, rows)
}

pub(crate) async fn fetch(
    client: &Client,
    role: &Role,
    credentials: &Credentials,
) -> Result<Vec<Pull>, String> {
    let permission = authorize(client, role, credentials).await?;
    let mut pulls = Vec::new();
    for (pool, label) in POOLS {
        let mut cursor = 0u64;
        let mut page = 1;
        loop {
            tokio::time::sleep(Duration::from_millis(400)).await;
            let results = history_page(client, role, &permission, pool, &cursor.to_string(), page)
                .await
                .map_err(|error| format!("{label}读取失败。{error}"))?;
            let Some(last) = results.last() else { break };
            let next = last
                .id
                .parse::<u64>()
                .map_err(|_| "祈愿记录的分页编号无效。")?;
            if next == 0 || (cursor != 0 && next >= cursor) {
                return Err("祈愿接口返回了重复分页，同步已停止。".into());
            }
            cursor = next;
            let has_more = results.len() >= 20;
            pulls.extend(results);
            if !has_more {
                break;
            }
            page += 1;
        }
    }
    Ok(pulls)
}
