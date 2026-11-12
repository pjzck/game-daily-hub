import { invoke } from "@tauri-apps/api/core";
import { CircleAlert, LayoutGrid, Link2, Monitor, Plus, RefreshCw, Sparkles, UsersRound } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { AccountCard, AccountDialog, Modal, NoAccounts } from "./Accounts";
import { Devices } from "./Devices";
import { Gacha } from "./Gacha";
import { errorMessage, games, type Account, type Game } from "./model";
import "./styles.css";

type View = "accounts" | "devices" | "gacha";
const pages = [
  { id: "accounts", title: "账号总览", Icon: LayoutGrid },
  { id: "gacha", title: "抽卡记录", Icon: Sparkles },
  { id: "devices", title: "MuMu 实例", Icon: Monitor },
] as const;

function Summary({ icon, label, count, tone }: { icon: ReactNode; label: string; count: number; tone: string }) {
  return <div className="summary-card">
    <span className={"summary-icon " + tone}>{icon}</span>
    <div><p>{label}</p><strong>{count}</strong></div>
  </div>;
}

function App() {
  const [view, setView] = useState<View>("accounts");
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [filter, setFilter] = useState<Game | "all">("all");
  const [loading, setLoading] = useState(true);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState<Account | null>();
  const [removing, setRemoving] = useState<Account>();
  const [removeBusy, setRemoveBusy] = useState(false);
  const [removeError, setRemoveError] = useState("");
  const displayed = filter === "all" ? accounts : accounts.filter(account => account.role.game === filter);
  const linked = accounts.filter(account => account.source === "miyoushe");
  const issues = linked.filter(account => account.issue);

  async function loadAccounts() {
    setLoading(true);
    setError("");
    try {
      setAccounts(await invoke<Account[]>("list_accounts"));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setLoading(false);
    }
  }
  useEffect(() => { void loadAccounts(); }, []);

  async function refresh() {
    setRefreshing(true);
    setError("");
    try {
      setAccounts(await invoke<Account[]>("refresh_accounts"));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setRefreshing(false);
    }
  }

  async function remove() {
    if (!removing) return;
    setRemoveBusy(true);
    setRemoveError("");
    try {
      setAccounts(await invoke<Account[]>("remove_account", { id: removing.id }));
      setRemoving(undefined);
      setFilter("all");
    } catch (reason) {
      setRemoveError(errorMessage(reason));
    } finally {
      setRemoveBusy(false);
    }
  }

  return <div className="shell">
    <aside className="sidebar">
      <div className="brand"><span className="brand-mark"><LayoutGrid size={22} /></span>
        <div><strong>Game Daily Hub</strong><span>游戏日常管理</span></div>
      </div>
      <nav aria-label="主导航">
        {pages.map(({ id, title, Icon }) => <button key={id} className="nav-button"
          aria-current={view === id ? "page" : undefined} onClick={() => setView(id)}>
          <Icon size={19} />{title}
        </button>)}
      </nav>
      <div className="sidebar-footer"><span className="indicator" />本地工作区</div>
    </aside>
    <main className="main">
      {view === "devices" ? <Devices /> : view === "gacha" ? <Gacha accounts={accounts} /> : <>
        <header className="page-header">
          <div><h1>账号总览</h1><p className="muted">角色资料与每日状态</p></div>
          <div className="page-actions">
            <button className="button secondary" onClick={refresh} disabled={!linked.length || loading || refreshing}>
              <RefreshCw size={17} className={refreshing ? "spin" : ""} />{refreshing ? "刷新中…" : "刷新全部"}
            </button>
            <button className="button primary" onClick={() => setEditing(null)} disabled={loading || refreshing}><Plus size={18} />添加账号</button>
          </div>
        </header>
        {error && <div className="notice error" role="alert">{error}<button className="text-button" onClick={loadAccounts}>重新读取</button></div>}
        <div className="summaries">
          <Summary icon={<UsersRound size={20} />} label="账号" count={accounts.length} tone="violet" />
          <Summary icon={<Link2 size={20} />} label="已连接米游社" count={linked.length} tone="teal" />
          <Summary icon={<CircleAlert size={20} />} label="待处理" count={issues.length} tone="amber" />
        </div>
        <div className="section-heading"><h2>账号卡片</h2>
          {accounts.length > 0 && <select aria-label="筛选游戏" value={filter}
            onChange={event => setFilter(event.target.value as Game | "all")}>
            <option value="all">全部游戏</option>
            {Object.entries(games).map(([key, game]) => <option value={key} key={key}>{game.name}</option>)}
          </select>}
        </div>
        {loading ? <p className="notice" role="status">正在读取本地账号…</p> :
          accounts.length === 0 ? <NoAccounts add={() => setEditing(null)} /> :
          displayed.length === 0 ? <p className="notice">该游戏还没有账号。</p> :
          <section className="account-grid" aria-label="账号列表" aria-busy={refreshing}>
            {displayed.map(account => <AccountCard key={account.id} account={account}
              onEdit={() => { if (!refreshing) setEditing(account); }}
              onRemove={() => { if (!refreshing) { setRemoveError(""); setRemoving(account); } }} />)}
          </section>}
      </>}
    </main>
    {editing !== undefined && <AccountDialog account={editing} close={() => setEditing(undefined)} saved={result => {
      setAccounts(result);
      setFilter("all");
      setError("");
      setEditing(undefined);
    }} />}
    {removing && <Modal title={removing.connection ? "断开米游社连接" : "移除账号"} busy={removeBusy}
      close={() => setRemoving(undefined)}>
      <p className="form-description">{removing.connection
        ? "将移除该连接的 " + accounts.filter(account => account.connection === removing.connection).length + " 张卡片及本地凭据。"
        : "移除「" + removing.role.name + "」的本地卡片？"}</p>
      {removeError && <p className="notice error" role="alert">{removeError}</p>}
      <div className="dialog-actions">
        <button className="button secondary" onClick={() => setRemoving(undefined)} disabled={removeBusy}>取消</button>
        <button className="button danger" onClick={remove} disabled={removeBusy}>{removing.connection ? "断开连接" : "移除账号"}</button>
      </div>
    </Modal>}
  </div>;
}

createRoot(document.getElementById("root")!).render(<App />);
