use crate::{community, protected};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{ErrorKind, Write},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Game {
    Genshin,
    Starrail,
    Arknights,
    Reverse1999,
}

#[derive(Clone, Copy, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Manual,
    Miyoushe,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Role {
    pub game: Game,
    pub uid: String,
    pub server: String,
    pub server_name: String,
    pub name: String,
    pub level: Option<u32>,
}

impl Role {
    fn matches(&self, other: &Self) -> bool {
        !self.uid.is_empty()
            && self.game == other.game
            && self.uid == other.uid
            && self.server == other.server
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Meter {
    pub current: u32,
    pub maximum: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notes {
    pub stamina: Option<Meter>,
    pub activity: Option<Meter>,
    pub full_at: Option<u64>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: Uuid,
    pub role: Role,
    pub source: Source,
    pub connection: Option<Uuid>,
    pub profile_at: u64,
    pub notes_at: Option<u64>,
    pub notes: Option<Notes>,
    pub issue: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualInput {
    id: Option<Uuid>,
    role: Role,
    stamina: Option<Meter>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Credentials {
    pub secret: String,
    pub device_id: String,
    pub device_extra: String,
}

#[derive(Default, Deserialize, Serialize)]
struct Workspace {
    accounts: Vec<Account>,
    credentials: BTreeMap<Uuid, Vec<u8>>,
}

pub struct AccountStore {
    path: PathBuf,
    operation: Mutex<()>,
    pub(crate) client: reqwest::Client,
}

impl AccountStore {
    pub fn open(directory: PathBuf) -> Result<Self, reqwest::Error> {
        Ok(Self {
            path: directory.join("accounts.json"),
            operation: Mutex::new(()),
            client: reqwest::Client::builder()
                .https_only(true)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(10))
                .user_agent("GameDailyHub/0.1")
                .build()?,
        })
    }

    fn read(&self) -> Result<Workspace, String> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "本地账号文件无法读取，请检查 accounts.json。".into()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Workspace::default()),
            Err(_) => Err("无法访问本地账号文件，请检查目录权限。".into()),
        }
    }

    fn save(&self, mut workspace: Workspace) -> Result<Vec<Account>, String> {
        workspace.credentials.retain(|id, _| {
            workspace
                .accounts
                .iter()
                .any(|entry| entry.connection == Some(*id))
        });
        let parent = self.path.parent().expect("account file has a parent");
        let error = |_| "无法保存本地账号，请检查目录权限和磁盘空间。".to_string();
        fs::create_dir_all(parent).map_err(error)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
        serde_json::to_writer(&mut file, &workspace)
            .map_err(|_| "无法写入本地账号。".to_string())?;
        file.flush().map_err(error)?;
        file.as_file().sync_all().map_err(error)?;
        file.persist(&self.path)
            .map_err(|_| "无法替换本地账号文件。".to_string())?;
        Ok(workspace.accounts)
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[tauri::command]
pub async fn list_accounts(store: State<'_, AccountStore>) -> Result<Vec<Account>, String> {
    let _guard = store.operation.lock().await;
    Ok(store.read()?.accounts)
}

#[tauri::command]
pub async fn save_manual_account(
    input: ManualInput,
    store: State<'_, AccountStore>,
) -> Result<Vec<Account>, String> {
    let _guard = store.operation.lock().await;
    let mut workspace = store.read()?;
    let mut role = input.role;
    role.name = role.name.trim().to_owned();
    role.uid = role.uid.trim().to_owned();
    role.server = role.server.trim().to_owned();
    role.server_name = role.server.clone();
    if role.name.is_empty() || role.name.chars().count() > 60 {
        return Err("请输入 1 到 60 个字符的账号名称。".into());
    }
    if role.uid.len() > 24 || !role.uid.bytes().all(|c| c.is_ascii_digit()) {
        return Err("UID 请输入数字。".into());
    }
    if role.server.chars().count() > 40 {
        return Err("区服名称不能超过 40 个字符。".into());
    }
    if input
        .stamina
        .as_ref()
        .is_some_and(|meter| meter.maximum == 0)
    {
        return Err("体力上限需要大于零。".into());
    }
    let id = input.id.unwrap_or_else(Uuid::new_v4);
    if workspace
        .accounts
        .iter()
        .any(|account| account.id != id && account.role.matches(&role))
    {
        return Err("该游戏、区服和 UID 已经存在。".into());
    }
    let account = Account {
        id,
        role,
        source: Source::Manual,
        connection: None,
        profile_at: now(),
        notes_at: input.stamina.as_ref().map(|_| now()),
        notes: input.stamina.map(|stamina| Notes {
            stamina: Some(stamina),
            activity: None,
            full_at: None,
        }),
        issue: None,
    };
    match input.id {
        Some(id) => {
            let existing = workspace
                .accounts
                .iter_mut()
                .find(|entry| entry.id == id && entry.source == Source::Manual)
                .ok_or("找不到要编辑的手动账号。")?;
            *existing = account;
        }
        None => workspace.accounts.push(account),
    }
    store.save(workspace)
}

#[tauri::command]
pub async fn remove_account(
    id: Uuid,
    store: State<'_, AccountStore>,
) -> Result<Vec<Account>, String> {
    let _guard = store.operation.lock().await;
    let mut workspace = store.read()?;
    let account = workspace
        .accounts
        .iter()
        .find(|account| account.id == id)
        .ok_or("账号已移除。")?;
    if let Some(connection) = account.connection {
        workspace
            .accounts
            .retain(|entry| entry.connection != Some(connection));
        workspace.credentials.remove(&connection);
    } else {
        workspace.accounts.retain(|entry| entry.id != id);
    }
    store.save(workspace)
}

#[tauri::command]
pub async fn connect_accounts(
    credentials: Credentials,
    existing_id: Option<Uuid>,
    store: State<'_, AccountStore>,
) -> Result<Vec<Account>, String> {
    connect(&store, credentials, existing_id).await
}

pub(crate) async fn connect(
    store: &AccountStore,
    mut credentials: Credentials,
    existing_id: Option<Uuid>,
) -> Result<Vec<Account>, String> {
    credentials.secret = credentials.secret.trim().to_owned();
    credentials.device_id = credentials.device_id.trim().to_owned();
    credentials.device_extra = credentials.device_extra.trim().to_owned();
    community::validate_credentials(&credentials)?;
    let source = Source::Miyoushe;

    let _guard = store.operation.lock().await;
    let mut workspace = store.read()?;
    let roles = community::roles(&store.client, &credentials).await?;
    if roles.is_empty() {
        return Err("社区账号下没有找到本期支持的已绑定角色。".into());
    }
    let connection = if let Some(id) = existing_id {
        let existing = workspace.accounts.iter().find(|entry| entry.id == id);
        let existing = existing.ok_or("找不到要更新连接的账号。")?;
        if existing.source != source || !roles.iter().any(|role| role.matches(&existing.role)) {
            return Err("这份凭据未返回要更新的角色，请检查登录账号。".into());
        }
        existing.connection.ok_or("该账号没有社区连接。")?
    } else {
        workspace
            .accounts
            .iter()
            .find(|entry| {
                entry.source == source && roles.iter().any(|role| role.matches(&entry.role))
            })
            .and_then(|entry| entry.connection)
            .unwrap_or_else(Uuid::new_v4)
    };
    let bytes = serde_json::to_vec(&credentials).map_err(|_| "无法保存凭据。")?;
    let encrypted = protected::transform(&bytes, true)?;
    apply_roles(
        &mut workspace,
        &store.client,
        connection,
        &credentials,
        roles,
    )
    .await;
    workspace.credentials.insert(connection, encrypted);
    store.save(workspace)
}

#[tauri::command]
pub async fn refresh_accounts(store: State<'_, AccountStore>) -> Result<Vec<Account>, String> {
    let _guard = store.operation.lock().await;
    let mut workspace = store.read()?;
    let connections: std::collections::BTreeSet<Uuid> = workspace
        .accounts
        .iter()
        .filter_map(|account| account.connection)
        .collect();
    if connections.is_empty() {
        return Ok(workspace.accounts);
    }
    for connection in connections {
        if let Err(error) = refresh_connection(&store.client, &mut workspace, connection).await {
            for account in workspace
                .accounts
                .iter_mut()
                .filter(|entry| entry.connection == Some(connection))
            {
                account.issue = Some(error.clone());
            }
        }
    }
    store.save(workspace)
}

async fn refresh_connection(
    client: &reqwest::Client,
    workspace: &mut Workspace,
    connection: Uuid,
) -> Result<(), String> {
    let encrypted = workspace
        .credentials
        .get(&connection)
        .ok_or("凭据缺失，请更新连接。")?;
    let credentials = serde_json::from_slice(&protected::transform(encrypted, false)?)
        .map_err(|_| "凭据无法读取，请更新连接。")?;
    let roles = community::roles(client, &credentials).await?;
    apply_roles(workspace, client, connection, &credentials, roles).await;
    Ok(())
}

async fn apply_roles(
    workspace: &mut Workspace,
    client: &reqwest::Client,
    connection: Uuid,
    credentials: &Credentials,
    roles: Vec<Role>,
) {
    for account in workspace
        .accounts
        .iter_mut()
        .filter(|entry| entry.connection == Some(connection))
    {
        account.issue = Some("本次未返回该角色，已保留上次数据。".into());
    }
    let profile_at = now();
    let genshin_roles = roles
        .iter()
        .filter(|role| role.game == Game::Genshin)
        .count();
    let starrail_roles = roles
        .iter()
        .filter(|role| role.game == Game::Starrail)
        .count();
    for role in roles {
        let single_role = match role.game {
            Game::Genshin => genshin_roles == 1,
            Game::Starrail => starrail_roles == 1,
            _ => false,
        };
        let result = community::notes(client, credentials, &role, single_role).await;
        let existing = workspace
            .accounts
            .iter_mut()
            .find(|entry| entry.role.matches(&role));
        let account = if let Some(entry) = existing {
            entry.role = role;
            entry
        } else {
            workspace.accounts.push(Account {
                id: Uuid::new_v4(),
                role,
                source: Source::Miyoushe,
                connection: Some(connection),
                profile_at,
                notes_at: None,
                notes: None,
                issue: None,
            });
            workspace.accounts.last_mut().expect("just pushed account")
        };
        if account.source == Source::Manual {
            account.notes = None;
            account.notes_at = None;
        }
        account.source = Source::Miyoushe;
        account.connection = Some(connection);
        account.profile_at = profile_at;
        match result {
            Ok(notes) => {
                account.notes = Some(notes);
                account.notes_at = Some(now());
                account.issue = None;
            }
            Err(error) => account.issue = Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "uses a local signed-in account and the live MiYouShe service"]
    fn refresh_live_miyoushe_accounts() {
        tauri::async_runtime::block_on(async {
            let directory = std::env::var_os("GAME_DAILY_HUB_DATA_DIR")
                .expect("set GAME_DAILY_HUB_DATA_DIR to the signed-in app's data directory");
            let store = AccountStore::open(directory.into()).expect("create HTTP client");
            let mut workspace = store.read().expect("read accounts");
            let connections: Vec<_> = workspace.credentials.keys().copied().collect();
            assert!(!connections.is_empty(), "no signed-in accounts");
            let mut complete = true;
            for connection in connections {
                refresh_connection(&store.client, &mut workspace, connection)
                    .await
                    .expect("refresh connected roles");
                for account in workspace
                    .accounts
                    .iter()
                    .filter(|entry| entry.connection == Some(connection))
                {
                    let game = account.role.game;
                    if let Some(issue) = &account.issue {
                        println!("{game:?}: {issue}");
                        complete = false;
                    } else {
                        let stamina = account
                            .notes
                            .as_ref()
                            .and_then(|notes| notes.stamina.as_ref())
                            .expect("returned stamina");
                        println!("{game:?}: {}/{}", stamina.current, stamina.maximum);
                    }
                }
            }
            assert!(complete, "some real-time notes could not be refreshed");
        });
    }
}
