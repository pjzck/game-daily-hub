import { invoke } from "@tauri-apps/api/core";
import { ChevronLeft, ChevronRight, Sparkles, Upload } from "lucide-react";
import { useEffect, useRef, useState, type ChangeEvent } from "react";
import { EmptyState, GameMark } from "./Accounts";
import { errorMessage, games, type Account } from "./model";

type GachaGame = "genshin" | "starrail";
export interface GachaTarget {
  game: GachaGame;
  uid: string;
}
interface Archive extends GachaTarget {
  total: number;
  fiveStar: number;
  knownRank: number;
  pools: Record<string, number>;
}
interface Pull {
  id: string;
  pool: string;
  name: string;
  rank: number | null;
  time: string;
}
interface Page {
  rows: Pull[];
  total: number;
}
interface ImportResult {
  archives: Archive[];
  added: number;
  duplicates: number;
  skipped: number;
}

const pools: Record<GachaGame, Record<string, string>> = {
  genshin: { "100": "新手祈愿", "200": "常驻祈愿", "301": "角色活动祈愿", "302": "武器活动祈愿", "500": "集录祈愿" },
  starrail: { "1": "常驻跃迁", "2": "始发跃迁", "11": "角色活动跃迁", "12": "光锥活动跃迁", "21": "角色联动跃迁", "22": "光锥联动跃迁" },
};
const archiveKey = (archive: GachaTarget) => archive.game + ":" + archive.uid;
const poolName = (game: GachaGame, pool: string) => pools[game][pool] ?? "卡池 " + pool;
const pageSize = 50;

export function Gacha({ accounts, target }: { accounts: Account[]; target: GachaTarget | null }) {
  const picker = useRef<HTMLInputElement>(null);
  const [archives, setArchives] = useState<Archive[]>([]);
  const [selected, setSelected] = useState(() => target ? archiveKey(target) : "");
  const [pool, setPool] = useState("");
  const [offset, setOffset] = useState(0);
  const [page, setPage] = useState<Page>({ rows: [], total: 0 });
  const [loading, setLoading] = useState(true);
  const [importing, setImporting] = useState(false);
  const [pageLoading, setPageLoading] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [revision, setRevision] = useState(0);
  const archive = selected ? archives.find(entry => archiveKey(entry) === selected) : archives[0];
  const missingTarget = target && !archives.some(entry => archiveKey(entry) === archiveKey(target)) ? target : null;
  const shown = archive ?? missingTarget;

  useEffect(() => {
    void invoke<Archive[]>("list_gacha").then(setArchives).catch(reason => setError(errorMessage(reason)))
      .finally(() => setLoading(false));
  }, []);

  const game = archive?.game;
  const uid = archive?.uid;
  useEffect(() => {
    if (!game || !uid) return;
    let active = true;
    setPageLoading(true);
    void invoke<Page>("gacha_page", { game, uid, pool: pool || null, offset })
      .then(value => { if (active) setPage(value); })
      .catch(reason => {
        if (active) {
          setPage({ rows: [], total: 0 });
          setError(errorMessage(reason));
        }
      })
      .finally(() => { if (active) setPageLoading(false); });
    return () => { active = false; };
  }, [game, uid, pool, offset, revision]);

  async function importFile(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    setImporting(true);
    setError("");
    setNotice("");
    try {
      if (file.size > 20 * 1024 * 1024) throw new Error("请选择 20 MB 以内的抽卡记录文件。");
      const result = await invoke<ImportResult>("import_gacha", { content: await file.text() });
      setArchives(result.archives);
      setOffset(0);
      setRevision(current => current + 1);
      setNotice(
        "新增 " + result.added + " 条，跳过 " + result.duplicates + " 条重复记录" +
        (result.skipped ? "，忽略 " + result.skipped + " 条无效记录。" : "。"),
      );
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setImporting(false);
    }
  }

  function archiveLabel(entry: GachaTarget) {
    const account = accounts.find(account => account.role.game === entry.game && account.role.uid === entry.uid);
    return games[entry.game].name + (account ? " · " + account.role.name : "") + " · UID " + entry.uid;
  }

  return <>
    <header className="page-header">
      <div><h1>抽卡记录</h1><p className="muted">原神祈愿与崩铁跃迁</p></div>
      <input type="file" accept=".json,application/json" ref={picker} onChange={importFile} hidden />
      <button className="button primary" onClick={() => picker.current?.click()} disabled={importing || loading}>
        <Upload size={17} />{importing ? "导入中…" : "导入记录"}
      </button>
    </header>
    {error && <p className="notice error" role="alert">{error}</p>}
    {notice && <p className="notice success" role="status">{notice}</p>}
    {!loading && shown && <div className="panel archive-header">
      <GameMark game={shown.game} />
      <label>账号
        <select value={archiveKey(shown)} onChange={event => {
          setSelected(event.target.value);
          setPool("");
          setOffset(0);
          setError("");
        }}>
          {missingTarget && <option value={archiveKey(missingTarget)}>{archiveLabel(missingTarget)}</option>}
          {archives.map(entry => <option key={archiveKey(entry)} value={archiveKey(entry)}>{archiveLabel(entry)}</option>)}
        </select>
      </label>
      {archive && <>
        <div className="archive-stat"><span>已存记录</span><strong>{archive.total}</strong></div>
        <div className="archive-stat"><span>已识别五星</span><strong>{archive.knownRank ? archive.fiveStar : "—"}</strong></div>
      </>}
    </div>}
    {archive ? (
        <div className="panel table-panel">
          <div className="panel-title">
            <h2>获得记录</h2>
            <select aria-label="筛选卡池" value={pool} onChange={event => { setPool(event.target.value); setOffset(0); }}>
              <option value="">全部卡池</option>
              {Object.entries(archive.pools).map(([key, count]) => <option key={key} value={key}>{poolName(archive.game, key)} · {count}</option>)}
            </select>
          </div>
          <table aria-busy={pageLoading}>
            <thead><tr><th>物品</th><th>卡池</th><th>稀有度</th><th>时间</th></tr></thead>
            <tbody>{pageLoading ? <tr><td colSpan={4} className="muted">正在读取记录…</td></tr> :
              page.rows.length ? page.rows.map(row => <tr key={row.id}>
                <td><strong className={row.rank === 5 ? "gold" : undefined}>{row.name}</strong></td>
                <td>{poolName(archive.game, row.pool)}</td>
                <td>{row.rank ? row.rank + " 星" : "—"}</td>
                <td className="mono">{row.time}</td>
              </tr>) : <tr><td colSpan={4} className="muted">当前没有记录</td></tr>}</tbody>
          </table>
          <div className="pagination">
            <span className="small muted">{page.total ? (offset + 1) + "–" + Math.min(offset + pageSize, page.total) + " / " + page.total : "0 条"}</span>
            <button className="icon-button" aria-label="上一页" disabled={!offset || pageLoading} onClick={() => setOffset(offset - pageSize)}><ChevronLeft size={18} /></button>
            <button className="icon-button" aria-label="下一页" disabled={offset + pageSize >= page.total || pageLoading} onClick={() => setOffset(offset + pageSize)}><ChevronRight size={18} /></button>
          </div>
        </div>
    ) : <EmptyState icon={<Sparkles size={29} />}
      title={loading ? "正在读取记录" : shown ? "该账号还没有抽卡记录" : "还没有抽卡记录"}
      detail={loading ? undefined : "选择原神 UIGF 或崩铁 UIGF／SRGF 的 JSON 导出文件。"} />}
  </>;
}
