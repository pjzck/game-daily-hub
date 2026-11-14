use crate::{
    accounts::{AccountStore, Game, Role},
    genshin_wishes, starrail_warps,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};
use tauri::State;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Pull {
    pub(crate) id: String,
    pool: String,
    name: String,
    rank: Option<u8>,
    time: String,
}

#[derive(Deserialize, Serialize)]
struct Archive {
    game: Game,
    uid: String,
    pulls: Vec<Pull>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    game: Game,
    uid: String,
    total: usize,
    five_star: usize,
    known_rank: usize,
    pools: BTreeMap<String, usize>,
}

#[derive(Serialize)]
pub struct Page {
    rows: Vec<Pull>,
    total: usize,
}

#[derive(Serialize)]
pub struct Imported {
    archives: Vec<Summary>,
    added: usize,
    duplicates: usize,
    skipped: usize,
}

pub struct GachaStore {
    path: PathBuf,
    operation: Mutex<()>,
    online_sync: Mutex<()>,
}

impl GachaStore {
    pub fn new(directory: &Path) -> Self {
        Self {
            path: directory.join("gacha.json"),
            operation: Mutex::new(()),
            online_sync: Mutex::new(()),
        }
    }

    fn read(&self) -> Result<Vec<Archive>, String> {
        match fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .map_err(|_| "本地抽卡文件无法读取，请检查 gacha.json。".into()),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Vec::new()),
            Err(_) => Err("无法访问本地抽卡文件。".into()),
        }
    }

    fn save(&self, archives: &[Archive]) -> Result<(), String> {
        let parent = self.path.parent().expect("archive file has a parent");
        let error = |_| "无法保存抽卡记录，请检查目录权限和磁盘空间。".to_string();
        fs::create_dir_all(parent).map_err(error)?;
        let mut file = tempfile::NamedTempFile::new_in(parent).map_err(error)?;
        serde_json::to_writer(&mut file, archives).map_err(|_| "无法写入抽卡记录。")?;
        file.flush().map_err(error)?;
        file.as_file().sync_all().map_err(error)?;
        file.persist(&self.path)
            .map_err(|_| "无法替换本地抽卡文件。")?;
        Ok(())
    }
}

fn summarize(archives: &[Archive]) -> Vec<Summary> {
    archives
        .iter()
        .map(|archive| {
            let mut pools = BTreeMap::new();
            let mut five_star = 0;
            let mut known_rank = 0;
            for pull in &archive.pulls {
                *pools.entry(pull.pool.clone()).or_insert(0) += 1;
                five_star += usize::from(pull.rank == Some(5));
                known_rank += usize::from(pull.rank.is_some());
            }
            Summary {
                game: archive.game,
                uid: archive.uid.clone(),
                total: archive.pulls.len(),
                five_star,
                known_rank,
                pools,
            }
        })
        .collect()
}

fn field(value: &Value, max: usize) -> Option<String> {
    if let Some(text) = value.as_str() {
        let text = text.trim();
        if !text.is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control) {
            return Some(text.to_owned());
        }
    }
    value.as_u64().map(|number| number.to_string())
}

fn pull(game: Game, row: &Value) -> Option<Pull> {
    let id = field(&row["id"], 80)?;
    let time = field(&row["time"], 40)?;
    let mut pool = field(&row["gacha_type"], 16).or_else(|| field(&row["uigf_gacha_type"], 16))?;
    if game == Game::Genshin && pool == "400" {
        pool = "301".to_owned();
    }
    let name = field(&row["name"], 120)
        .or_else(|| field(&row["item_name"], 120))
        .or_else(|| field(&row["item_id"], 20).map(|id| format!("物品 {id}")))?;
    let rank = field(&row["rank_type"], 2)
        .and_then(|value| value.parse::<u8>().ok())
        .filter(|rank| (1..=5).contains(rank));
    Some(Pull {
        id,
        pool,
        name,
        rank,
        time,
    })
}

fn parse_export(data: &Value) -> Result<(Vec<Archive>, usize), String> {
    let mut sections = Vec::new();
    for (key, game) in [("hk4e", Game::Genshin), ("hkrpg", Game::Starrail)] {
        if let Some(roles) = data[key].as_array() {
            sections.extend(roles.iter().map(|role| (game, &role["uid"], &role["list"])));
        }
    }
    if data["list"].is_array() {
        let info = &data["info"];
        let game = if info["srgf_version"].is_string() {
            Some(Game::Starrail)
        } else if info["uigf_version"].is_string() {
            Some(Game::Genshin)
        } else {
            None
        };
        if let Some(game) = game {
            sections.push((game, &info["uid"], &data["list"]));
        }
    }
    let mut archives = Vec::new();
    let mut skipped = 0;
    for (game, uid, records) in sections {
        let Some(records) = records.as_array() else {
            continue;
        };
        let Some(uid) =
            field(uid, 24).filter(|uid| uid.len() <= 24 && uid.bytes().all(|c| c.is_ascii_digit()))
        else {
            skipped += records.len();
            continue;
        };
        let pulls: Vec<_> = records
            .iter()
            .filter_map(|entry| pull(game, entry))
            .collect();
        skipped += records.len() - pulls.len();
        if !pulls.is_empty() {
            archives.push(Archive { game, uid, pulls });
        }
    }
    if archives.is_empty() {
        return Err("未找到有效记录。请导入原神 UIGF 或崩铁 UIGF／SRGF 的 JSON 导出文件。".into());
    }
    Ok((archives, skipped))
}

#[tauri::command]
pub async fn list_gacha(store: State<'_, GachaStore>) -> Result<Vec<Summary>, String> {
    let _guard = store.operation.lock().await;
    Ok(summarize(&store.read()?))
}

#[tauri::command]
pub async fn import_gacha(
    content: String,
    store: State<'_, GachaStore>,
) -> Result<Imported, String> {
    if content.len() > 20 * 1024 * 1024 {
        return Err("请选择 20 MB 以内的抽卡记录文件。".into());
    }
    let parsed = serde_json::from_str(content.trim_start_matches('\u{feff}'))
        .map_err(|_| "无法解析 JSON 文件。")?;
    let (imported, skipped) = parse_export(&parsed)?;
    merge_archives(&store, imported, skipped).await
}

#[tauri::command]
pub async fn sync_genshin_wishes(
    id: Uuid,
    accounts: State<'_, AccountStore>,
    store: State<'_, GachaStore>,
) -> Result<Imported, String> {
    sync_wishes(&accounts, &store, id).await
}

pub(crate) async fn sync_wishes(
    accounts: &AccountStore,
    store: &GachaStore,
    id: Uuid,
) -> Result<Imported, String> {
    let _sync = store
        .online_sync
        .try_lock()
        .map_err(|_| "已有祈愿正在同步，请稍后再试。")?;
    let (role, credentials) = accounts.linked_session(id).await?;
    if role.game != Game::Genshin {
        return Err("同步祈愿适用于已连接米游社的原神账号。".into());
    }
    let pulls = genshin_wishes::fetch(&accounts.client, &role, &credentials).await?;
    let archive = Archive {
        game: Game::Genshin,
        uid: role.uid,
        pulls,
    };
    merge_archives(store, vec![archive], 0).await
}

#[tauri::command]
pub async fn sync_starrail_warps(
    uid: String,
    accounts: State<'_, AccountStore>,
    store: State<'_, GachaStore>,
) -> Result<Imported, String> {
    sync_warps(&accounts.client, &store, &uid).await
}

async fn sync_warps(
    client: &reqwest::Client,
    store: &GachaStore,
    uid: &str,
) -> Result<Imported, String> {
    if uid.is_empty() || uid.len() > 24 || !uid.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("请选择有效的崩铁 UID。".into());
    }
    let _sync = store
        .online_sync
        .try_lock()
        .map_err(|_| "已有抽卡记录正在同步，请稍后再试。")?;
    let history = tauri::async_runtime::spawn_blocking(starrail_warps::find_history)
        .await
        .map_err(|_| "无法读取崩铁客户端的跃迁历史缓存。")??;
    let pulls = starrail_warps::fetch(client, &history, uid).await?;
    let archive = Archive {
        game: Game::Starrail,
        uid: uid.to_owned(),
        pulls,
    };
    merge_archives(store, vec![archive], 0).await
}

pub(crate) fn wish_page(role: &Role, rows: &[Value]) -> Result<Vec<Pull>, String> {
    online_page(Game::Genshin, &role.uid, rows)
}

pub(crate) fn online_page(game: Game, uid: &str, rows: &[Value]) -> Result<Vec<Pull>, String> {
    rows.iter()
        .map(|row| {
            if field(&row["uid"], 24).as_deref() != Some(uid) {
                return Err(
                    "抽卡记录的 UID 与所选账号不一致，请在游戏中打开正确账号的历史记录。".into(),
                );
            }
            pull(game, row)
                .filter(|pull| pull.id.bytes().all(|ch| ch.is_ascii_digit()))
                .ok_or_else(|| "接口返回了无法识别的抽卡记录，同步已停止。".into())
        })
        .collect()
}

async fn merge_archives(
    store: &GachaStore,
    imported: Vec<Archive>,
    skipped: usize,
) -> Result<Imported, String> {
    let _guard = store.operation.lock().await;
    let mut archives = store.read()?;
    let mut added = 0;
    let mut duplicates = 0;
    for incoming in imported
        .into_iter()
        .filter(|archive| !archive.pulls.is_empty())
    {
        let archive = match archives
            .iter()
            .position(|entry| entry.game == incoming.game && entry.uid == incoming.uid)
        {
            Some(index) => &mut archives[index],
            None => {
                archives.push(Archive {
                    game: incoming.game,
                    uid: incoming.uid,
                    pulls: Vec::new(),
                });
                archives.last_mut().expect("just pushed archive")
            }
        };
        let mut ids: HashSet<String> = archive.pulls.iter().map(|pull| pull.id.clone()).collect();
        for pull in incoming.pulls {
            if ids.insert(pull.id.clone()) {
                archive.pulls.push(pull);
                added += 1;
            } else {
                duplicates += 1;
            }
        }
        archive.pulls.sort_by(|a, b| {
            b.time
                .cmp(&a.time)
                .then_with(|| b.id.len().cmp(&a.id.len()))
                .then_with(|| b.id.cmp(&a.id))
        });
    }
    if added > 0 {
        store.save(&archives)?;
    }
    Ok(Imported {
        archives: summarize(&archives),
        added,
        duplicates,
        skipped,
    })
}

#[cfg(test)]
pub(crate) async fn check_wish_sync(accounts: &AccountStore, id: Uuid) -> (usize, usize) {
    let directory = tempfile::tempdir().expect("isolated wish archive");
    let store = GachaStore::new(directory.path());
    let (role, _) = accounts.linked_session(id).await.expect("read linked role");
    let imported = serde_json::json!({"hk4e": [{ "uid": role.uid, "list": [{
        "id": "1", "gacha_type": "301", "name": "本地历史示例", "time": "2020-10-01 00:00:00"
    }]}]});
    let (archives, skipped) = parse_export(&imported).expect("parse existing history");
    merge_archives(&store, archives, skipped)
        .await
        .expect("seed local history");
    let first = sync_wishes(accounts, &store, id)
        .await
        .expect("first live wish sync");
    let second = sync_wishes(accounts, &store, id)
        .await
        .expect("second live wish sync");
    assert_eq!(second.added, 0);
    assert_eq!(second.duplicates, first.added + first.duplicates);
    let saved = store.read().expect("read merged wish history");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].uid, role.uid);
    assert_eq!(saved[0].pulls.len(), first.added + 1);
    assert!(saved[0].pulls.iter().any(|pull| pull.id == "1"));
    (first.added, second.duplicates)
}

#[tauri::command]
pub async fn gacha_page(
    game: Game,
    uid: String,
    pool: Option<String>,
    offset: usize,
    store: State<'_, GachaStore>,
) -> Result<Page, String> {
    let _guard = store.operation.lock().await;
    let archives = store.read()?;
    let archive = archives
        .iter()
        .find(|entry| entry.game == game && entry.uid == uid);
    let Some(archive) = archive else {
        return Ok(Page {
            rows: Vec::new(),
            total: 0,
        });
    };
    let matching = || {
        archive
            .pulls
            .iter()
            .filter(|pull| pool.as_ref().is_none_or(|pool| &pull.pool == pool))
    };
    Ok(Page {
        rows: matching().skip(offset).take(50).cloned().collect(),
        total: matching().count(),
    })
}

#[cfg(test)]
mod starrail_tests {
    use super::*;

    #[test]
    #[ignore = "requires an open Star Rail PC warp history and uses the official live service"]
    fn live_starrail_warp_sync() {
        tauri::async_runtime::block_on(async {
            let directory = tempfile::tempdir().expect("temporary data directory");
            let accounts = AccountStore::open(directory.path().into()).expect("HTTP client");
            let store = GachaStore::new(directory.path());
            let uid = starrail_warps::current_uid(&accounts.client)
                .await
                .expect("live history record UID");
            let first = sync_warps(&accounts.client, &store, &uid)
                .await
                .expect("first live warp sync");
            assert!(first.added > 0, "test requires at least one warp record");
            let second = sync_warps(&accounts.client, &store, &uid)
                .await
                .expect("repeat live warp sync");
            assert_eq!(second.added, 0, "repeat sync must not duplicate records");
            assert!(second.duplicates >= first.added);
            let archives = store.read().expect("persisted warp archive");
            assert!(
                archives.len() == 1 && archives[0].game == Game::Starrail && archives[0].uid == uid
            );
            assert_eq!(archives[0].pulls.len(), first.added);
            println!(
                "Star Rail: {} saved, {} duplicates on repeat, all pages matched the selected UID",
                first.added, second.duplicates
            );
        });
    }
}
