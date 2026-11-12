use crate::{
    accounts::{self, Account, AccountStore, Credentials},
    community::{self, passport_ds},
};
use qrcode::{QrCode, types::Color};
use reqwest::{Client, Url};
use serde::Serialize;
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

const CREATE_URL: &str = "https://passport-api.mihoyo.com/account/ma-cn-passport/app/createQRLogin";
const QUERY_URL: &str =
    "https://passport-api.mihoyo.com/account/ma-cn-passport/app/queryQRLoginStatus";
const QR_LIFETIME: Duration = Duration::from_secs(180);

#[derive(Default)]
pub struct QrLogin {
    pending: Mutex<Option<Session>>,
}

struct Session {
    id: Uuid,
    device: String,
    ticket: String,
    deadline: Instant,
    existing_id: Option<Uuid>,
    credentials: Option<Credentials>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginQr {
    id: Uuid,
    width: usize,
    modules: Vec<bool>,
    expires_in_seconds: u64,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginStatus {
    Waiting,
    Scanned,
    Confirmed,
    Expired,
    Canceled,
}

async fn passport_request(
    client: &Client,
    endpoint: &str,
    device: &str,
    body: String,
) -> Result<Value, String> {
    const VERSION: &str = "2.90.1";
    community::response(
        client
            .post(endpoint)
            .header("content-type", "application/json")
            .header("user-agent", "Mozilla/5.0 miHoYoBBS/2.90.1 Capture/2.2.0")
            .header("x-rpc-app_id", "bll8iq97cem8")
            .header("x-rpc-client_type", "3")
            .header("x-rpc-game_biz", "bbs_cn")
            .header("x-rpc-app_version", VERSION)
            .header("x-rpc-sdk_version", VERSION)
            .header("x-rpc-account_version", VERSION)
            .header("x-rpc-device_id", device)
            .header("ds", passport_ds(&body, ""))
            .body(body),
    )
    .await
}

fn qr_fields(data: &Value) -> Result<(Url, String), String> {
    let raw_url = data["url"].as_str().ok_or("米游社未返回二维码。")?;
    let url = Url::parse(raw_url).map_err(|_| "米游社返回的二维码地址无效。")?;
    if url.scheme() != "https"
        || url.host_str() != Some("user.mihoyo.com")
        || url.path() != "/login-platform/mobile.html"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err("二维码地址不属于米哈游通行证。".into());
    }
    let embedded = url
        .query_pairs()
        .find(|(key, _)| key == "tk")
        .map(|(_, value)| value.into_owned());
    let ticket = data["ticket"]
        .as_str()
        .map(str::to_owned)
        .or_else(|| embedded.clone())
        .filter(|ticket| !ticket.is_empty() && ticket.len() <= 512)
        .ok_or("米游社未返回登录会话。")?;
    if embedded.is_some_and(|value| value != ticket) {
        return Err("二维码与登录会话不匹配。".into());
    }
    Ok((url, ticket))
}

fn confirmed_credentials(data: &Value, device: &str) -> Result<Credentials, String> {
    let uid = community::text(&data["user_info"]["aid"])
        .or_else(|| community::text(&data["user_info"]["uid"]))
        .or_else(|| community::text(&data["user_info"]["account_id"]))
        .filter(|uid| !uid.is_empty() && uid.bytes().all(|ch| ch.is_ascii_digit()))
        .ok_or("扫码已确认，但米游社未返回账号标识。")?;
    let mid = data["user_info"]["mid"]
        .as_str()
        .filter(|value| community::valid_cookie_value(value))
        .ok_or("扫码已确认，但米游社未返回会话标识。")?;
    let token = data["tokens"]
        .as_array()
        .and_then(|tokens| {
            tokens
                .iter()
                .find(|token| community::number(&token["token_type"]) == Some(1))
        })
        .and_then(|token| token["token"].as_str())
        .filter(|value| value.starts_with("v2_") && community::valid_cookie_value(value))
        .ok_or("扫码已确认，但米游社未返回可用会话。")?;
    Ok(Credentials {
        secret: format!(
            "stoken={token}; mid={mid}; stuid={uid}; account_id_v2={uid}; account_mid_v2={mid}"
        ),
        device_id: device.to_owned(),
        device_extra: String::new(),
    })
}

#[tauri::command]
pub async fn start_miyoushe_login(
    existing_id: Option<Uuid>,
    login: State<'_, QrLogin>,
    store: State<'_, AccountStore>,
) -> Result<LoginQr, String> {
    let mut pending = login.pending.lock().await;
    *pending = None;
    let device = Uuid::new_v4().to_string().to_uppercase();
    let payload = passport_request(&store.client, CREATE_URL, &device, "{}".into()).await?;
    let (url, ticket) = qr_fields(&community::data(payload)?)?;
    let qr = QrCode::new(url.as_str()).map_err(|_| "无法生成米游社二维码。")?;
    let id = Uuid::new_v4();
    let image = LoginQr {
        id,
        width: qr.width(),
        modules: qr
            .into_colors()
            .into_iter()
            .map(|color| color == Color::Dark)
            .collect(),
        expires_in_seconds: QR_LIFETIME.as_secs(),
    };
    *pending = Some(Session {
        id,
        device,
        ticket,
        deadline: Instant::now() + QR_LIFETIME,
        existing_id,
        credentials: None,
    });
    Ok(image)
}

#[tauri::command]
pub async fn poll_miyoushe_login(
    id: Uuid,
    login: State<'_, QrLogin>,
    store: State<'_, AccountStore>,
) -> Result<LoginStatus, String> {
    let mut pending = login.pending.lock().await;
    let Some(session) = pending.as_mut().filter(|session| session.id == id) else {
        return Ok(LoginStatus::Canceled);
    };
    if Instant::now() >= session.deadline {
        *pending = None;
        return Ok(LoginStatus::Expired);
    }
    if session.credentials.is_some() {
        return Ok(LoginStatus::Confirmed);
    }
    let body = json!({ "ticket": session.ticket }).to_string();
    let payload = passport_request(&store.client, QUERY_URL, &session.device, body).await?;
    if community::retcode(&payload) == Some(-106) {
        *pending = None;
        return Ok(LoginStatus::Expired);
    }
    let data = community::data(payload)?;
    match data["status"].as_str() {
        Some("Created" | "Init") => Ok(LoginStatus::Waiting),
        Some("Scanned") => Ok(LoginStatus::Scanned),
        Some("Confirmed") => {
            session.credentials = Some(confirmed_credentials(&data, &session.device)?);
            session.deadline = Instant::now() + QR_LIFETIME;
            Ok(LoginStatus::Confirmed)
        }
        Some("Expired") => {
            *pending = None;
            Ok(LoginStatus::Expired)
        }
        Some("Canceled" | "Cancelled") => {
            *pending = None;
            Ok(LoginStatus::Canceled)
        }
        _ => Err("无法识别米游社的扫码状态。".into()),
    }
}

#[tauri::command]
pub async fn finish_miyoushe_login(
    id: Uuid,
    login: State<'_, QrLogin>,
    store: State<'_, AccountStore>,
) -> Result<Vec<Account>, String> {
    let mut pending = login.pending.lock().await;
    let session = pending
        .as_mut()
        .filter(|session| session.id == id && Instant::now() < session.deadline)
        .ok_or("扫码会话已结束，请重新生成二维码。")?;
    let credentials = session
        .credentials
        .as_mut()
        .ok_or("请先在米游社确认登录。")?;
    community::prepare_session(&store.client, credentials).await;
    let accounts = accounts::connect(&store, credentials.clone(), session.existing_id).await?;
    *pending = None;
    Ok(accounts)
}

#[tauri::command]
pub async fn cancel_miyoushe_login(id: Uuid, login: State<'_, QrLogin>) -> Result<(), String> {
    let mut pending = login.pending.lock().await;
    if pending.as_ref().is_some_and(|session| session.id == id) {
        *pending = None;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_uses_matching_passport_ticket() {
        let url = "https://user.mihoyo.com/login-platform/mobile.html?tk=example";
        assert!(qr_fields(&json!({ "url": url, "ticket": "example" })).is_ok());
        assert!(qr_fields(&json!({ "url": url, "ticket": "other" })).is_err());
        assert!(
            qr_fields(&json!({ "url": url.replace("user.mihoyo.com", "example.org") })).is_err()
        );
    }

    #[test]
    #[ignore = "uses the live Passport service, without confirming a login"]
    fn passport_returns_a_scannable_qr_and_waits() {
        tauri::async_runtime::block_on(async {
            let store = AccountStore::open(std::env::temp_dir()).expect("create HTTP client");
            let device = Uuid::new_v4().to_string().to_uppercase();
            let created = passport_request(&store.client, CREATE_URL, &device, "{}".into())
                .await
                .and_then(community::data)
                .expect("create QR");
            let (url, ticket) = qr_fields(&created).expect("validate QR URL and ticket");
            assert!(QrCode::new(url.as_str()).is_ok());
            let body = json!({ "ticket": ticket }).to_string();
            let state = passport_request(&store.client, QUERY_URL, &device, body)
                .await
                .and_then(community::data)
                .expect("query QR");
            assert!(matches!(state["status"].as_str(), Some("Created" | "Init")));
        });
    }
}
