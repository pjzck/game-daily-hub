import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronUp, CircleAlert, Eye, EyeOff, GripVertical, Link2, Pencil, Plus, RefreshCw, ScanLine, Sparkles, Star, StickyNote, Trash2, Trophy, UserRound, X } from "lucide-react";
import { useEffect, useRef, useState, type DragEvent, type FormEvent, type ReactNode } from "react";
import { dateTime, errorMessage, games, type Account, type Game, type Meter } from "./model";
import { MiyousheQr } from "./MiyousheQr";

export function GameMark({ game }: { game: Game }) {
  return <span className={"game-mark " + game} aria-hidden="true">{games[game].mark}</span>;
}

function ResourceMeter({ label, meter }: { label: string; meter: Meter | null }) {
  return (
    <div className="resource">
      <div className="resource-value">
        <span>{label}</span>
        {meter ? (
          <span><strong>{meter.current}</strong><span className="muted"> / {meter.maximum}</span></span>
        ) : <span className="muted">暂无数据</span>}
      </div>
      {meter && <progress aria-label={label} value={Math.min(meter.current, meter.maximum)} max={meter.maximum} />}
    </div>
  );
}

interface CardOrder {
  earlier?: () => void;
  later?: () => void;
  dragging: boolean;
  dropPosition?: "before" | "after";
  startDrag: () => void;
  endDrag: () => void;
  dragOver?: () => void;
  drop: (source: string) => void;
}

export function AccountCard({ account, busy, refreshing, order, onFollow, onEdit, onRemove, onGacha, onBattle, onRefresh, onMemo, onVisibility }: {
  account: Account;
  busy: boolean;
  refreshing: boolean;
  order?: CardOrder;
  onFollow: () => void;
  onEdit: () => void;
  onRemove: () => void;
  onGacha?: () => void;
  onBattle?: () => void;
  onRefresh?: () => void;
  onMemo: () => void;
  onVisibility: () => void;
}) {
  const { role, notes, issue } = account;
  const game = games[role.game];
  const manual = account.source === "manual";
  const assignmentLabel = role.game === "genshin" ? "探索派遣" : "委托派遣";
  const status = manual ? "手动记录" : issue ? (notes ? "数据待更新" : "待处理") : "已同步";
  const details = [role.serverName, role.uid && "UID " + role.uid, role.level !== null && "Lv. " + role.level].filter(Boolean);

  function acceptDrop(event: DragEvent<HTMLElement>) {
    if (!order?.dragOver || busy) return;
    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    order.dragOver();
  }

  return (
    <article className="account-card" data-dragging={order?.dragging || undefined}
      data-drop-position={order?.dropPosition} onDragOver={acceptDrop} onDrop={event => {
        if (!order?.dragOver || busy) return;
        event.preventDefault();
        order.drop(event.dataTransfer.getData("text/plain"));
      }}>
      <div className="account-heading">
        {order && !busy && <span className="card-drag-handle" title="拖动卡片调整顺序" draggable
          onDragStart={event => {
            event.dataTransfer.effectAllowed = "move";
            event.dataTransfer.setData("text/plain", account.id);
            const card = event.currentTarget.closest("article");
            if (card) event.dataTransfer.setDragImage(card, 24, 24);
            order.startDrag();
          }} onDragEnd={order.endDrag}><GripVertical size={17} aria-hidden="true" /></span>}
        <GameMark game={role.game} />
        <div className="account-identity">
          <p className="game-name">{game.name}</p>
          <h3 title={role.name}>{role.name}</h3>
        </div>
        <span className={"status-pill " + (issue ? "warning" : manual ? "neutral" : "good")}>{status}</span>
        <button className="icon-button follow-button" type="button" aria-pressed={account.followed}
          aria-label={"关注「" + role.name + "」"} title={account.followed ? "取消关注" : "关注账号"}
          disabled={busy} onClick={onFollow}>
          <Star size={17} fill={account.followed ? "currentColor" : "none"} />
        </button>
      </div>
      <p className="role-detail">{details.join(" · ") || "未填写区服与 UID"}</p>
      {account.memo && <button type="button" className="card-memo" onClick={onMemo} disabled={busy}
        title="编辑备注" aria-label={`编辑备注：${account.memo}`}>
        <StickyNote size={15} aria-hidden="true" />
        <span>{account.memo}</span>
      </button>}
      <div className="account-meters">
        <ResourceMeter label={game.stamina} meter={notes?.stamina ?? null} />
        {notes?.activity && <ResourceMeter label={game.activity} meter={notes.activity} />}
        {notes?.fullAt && notes.stamina && notes.stamina.current < notes.stamina.maximum ? (
          <p className="small muted">预计 {dateTime.format(notes.fullAt * 1000)} 回满</p>
        ) : null}
        {notes?.assignments && <div>
          <div className="resource-value"><span>{assignmentLabel}</span>
            <span><strong>{notes.assignments.current}</strong><span className="muted"> / {notes.assignments.maximum}</span></span>
          </div>
          {!!notes.assignmentsReady && <p className="small assignment-ready">{notes.assignmentsReady} 项可领取</p>}
        </div>}
        {notes?.homeCoin && <ResourceMeter label="洞天宝钱" meter={notes.homeCoin} />}
        {notes?.reserveStamina != null && <div className="resource-value">
          <span>储备开拓力</span><span><strong>{notes.reserveStamina}</strong>{notes.reserveFull && <span className="muted"> · 已满</span>}</span>
        </div>}
        {notes?.weeklyActivity && <ResourceMeter label="周活跃度" meter={notes.weeklyActivity} />}
        {notes?.simulatedUniverse && <ResourceMeter label="模拟宇宙积分" meter={notes.simulatedUniverse} />}
      </div>
      {issue && <p className="card-issue"><CircleAlert size={15} />{issue}</p>}
      <div className="card-links">
        {onRefresh && <button type="button" className="text-button" disabled={busy} onClick={onRefresh}
          aria-label={refreshing ? "正在刷新「" + role.name + "」" : "刷新「" + role.name + "」"}>
          <RefreshCw size={15} className={refreshing ? "spin" : undefined} />{refreshing ? "刷新中…" : "刷新"}
        </button>}
        {onBattle && <button type="button" className="text-button" onClick={onBattle}>
          <Trophy size={15} />战绩
        </button>}
        {onGacha && <button type="button" className="text-button" onClick={onGacha}>
          <Sparkles size={15} />{role.game === "genshin" ? "祈愿记录" : "跃迁记录"}
        </button>}
        {!account.memo && <button type="button" className="text-button" disabled={busy} onClick={onMemo}>
          <StickyNote size={15} />备注
        </button>}
        <button type="button" className="text-button" disabled={busy} onClick={onVisibility}
          aria-label={`${account.hidden ? "显示" : "隐藏"}「${role.name}」`}>
          {account.hidden ? <Eye size={15} /> : <EyeOff size={15} />}{account.hidden ? "恢复显示" : "隐藏"}
        </button>
      </div>
      <div className="account-footer">
        <div className="small muted">
          <div>{manual ? "本地记录" : "米游社"} · 资料 {dateTime.format(account.profileAt * 1000)}</div>
          {account.notesAt && !manual && <div>便笺 {dateTime.format(account.notesAt * 1000)}</div>}
        </div>
        <div className="icon-actions">
          {order && <>
            <button className="icon-button" type="button" onClick={order.earlier} disabled={busy || !order.earlier}
              title="上移" aria-label={"上移「" + role.name + "」"}><ChevronUp size={19} /></button>
            <button className="icon-button" type="button" onClick={order.later} disabled={busy || !order.later}
              title="下移" aria-label={"下移「" + role.name + "」"}><ChevronDown size={19} /></button>
          </>}
          <button className="icon-button" type="button" onClick={onEdit} disabled={busy}
            title={manual ? "编辑账号" : "更新连接"} aria-label={manual ? "编辑账号" : "更新连接"}>
            {manual ? <Pencil size={17} /> : <Link2 size={17} />}
          </button>
          <button className="icon-button delete-button" type="button" onClick={onRemove} disabled={busy}
            title={manual ? "移除账号" : "断开连接"} aria-label={manual ? "移除账号" : "断开连接"}>
            <Trash2 size={17} />
          </button>
        </div>
      </div>
    </article>
  );
}

export function EmptyState({ icon, title, detail, action }: {
  icon: ReactNode; title: string; detail?: string; action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      {detail && <p className="muted">{detail}</p>}
      {action}
    </div>
  );
}

export function Modal({ title, busy, close, children }: {
  title: string; busy: boolean; close: () => void; children: ReactNode;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return (
    <dialog ref={dialog} aria-label={title} className="dialog" onCancel={(event) => {
      event.preventDefault();
      if (!busy) close();
    }}>
      <div className="dialog-header">
        <h2>{title}</h2>
        <button type="button" className="icon-button" disabled={busy} aria-label="关闭" onClick={close}><X size={20} /></button>
      </div>
      {children}
    </dialog>
  );
}

export function AccountMemoDialog({ account, close, saved }: {
  account: Account;
  close: () => void;
  saved: (accounts: Account[]) => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (busy) return;
    const memo = String(new FormData(event.currentTarget).get("memo") ?? "");
    setError("");
    setBusy(true);
    try {
      saved(await invoke<Account[]>("update_account_card", { id: account.id, change: { type: "memo", memo } }));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return <Modal title={`备注 · ${account.role.name}`} busy={busy} close={close}>
    <form onSubmit={save} aria-busy={busy}>
      <label>账号备注<textarea name="memo" className="memo-input" rows={4} maxLength={240}
        defaultValue={account.memo} disabled={busy} autoFocus placeholder="例如：主号、周末清体力" /></label>
      {error && <p role="alert" className="notice error">{error}</p>}
      <div className="dialog-actions">
        <button type="button" className="button secondary" onClick={close} disabled={busy}>取消</button>
        <button type="submit" className="button primary" disabled={busy}>保存备注</button>
      </div>
    </form>
  </Modal>;
}

export function AccountDialog({ account, close, saved }: {
  account: Account | null;
  close: () => void;
  saved: (accounts: Account[]) => void;
}) {
  const [mode, setMode] = useState<"qr" | "miyoushe" | "manual">(account?.source === "manual" ? "manual" : "qr");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const role = account?.role;
  const stamina = account?.notes?.stamina;

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = event.currentTarget;
    const fields = new FormData(form);
    const value = (name: string) => String(fields.get(name) ?? "").trim();
    setBusy(true);
    setError("");
    try {
      let accounts: Account[];
      if (mode === "miyoushe") {
        accounts = await invoke<Account[]>("connect_accounts", {
          existingId: account?.id ?? null,
          credentials: {
            secret: value("secret"),
            deviceId: value("deviceId"),
            deviceExtra: value("deviceExtra"),
          },
        });
      } else {
        const current = value("current");
        const maximum = value("maximum");
        if (Boolean(current) !== Boolean(maximum)) throw new Error("请同时填写当前体力和体力上限。");
        accounts = await invoke<Account[]>("save_manual_account", {
          input: {
            id: account?.id ?? null,
            role: {
              game: value("game"),
              name: value("name"),
              uid: value("uid"),
              server: value("server"),
              serverName: value("server"),
              level: value("level") ? Number(value("level")) : null,
            },
            stamina: current ? { current: Number(current), maximum: Number(maximum) } : null,
          },
        });
      }
      form.reset();
      saved(accounts);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal title={account ? (account.source === "manual" ? "编辑账号" : "更新米游社连接") : "添加账号"} busy={busy} close={close}>
      {account?.source !== "manual" && <div className="segments" aria-label="账号来源">
        <button type="button" aria-pressed={mode === "qr"} onClick={() => { setMode("qr"); setError(""); }} disabled={busy}><ScanLine size={16} /> 米游社扫码</button>
        <button type="button" aria-pressed={mode === "miyoushe"} onClick={() => { setMode("miyoushe"); setError(""); }} disabled={busy}><Link2 size={16} /> 导入 Cookie</button>
        {!account && <button type="button" aria-pressed={mode === "manual"} onClick={() => { setMode("manual"); setError(""); }} disabled={busy}><Pencil size={16} /> 手动录入</button>}
      </div>}
      {mode === "qr" ? <MiyousheQr existingId={account?.id ?? null} setBusy={setBusy} saved={saved} /> : <form onSubmit={submit} key={mode} aria-busy={busy}>
        <fieldset disabled={busy}>
          {mode === "miyoushe" ? <>
            <p className="form-description">导入已有的米游社凭据，读取已绑定的原神和崩铁国服角色。凭据由 Windows 加密保存。</p>
            <label>米游社 Cookie<input name="secret" type="password" required autoComplete="off" maxLength={16384} /></label>
            <div className="field-grid">
              <label>设备 ID <span>选填</span><input name="deviceId" type="password" autoComplete="off" maxLength={256} /></label>
              <label>设备指纹 <span>选填</span><input name="deviceExtra" type="password" autoComplete="off" maxLength={256} /></label>
            </div>
          </> : <>
            <div className="field-grid">
              <label>游戏<select name="game" defaultValue={role?.game ?? "genshin"}>
                {Object.entries(games).map(([key, game]) => <option key={key} value={key}>{game.name}</option>)}
              </select></label>
              <label>账号名称<input name="name" required maxLength={60} defaultValue={role?.name} /></label>
              <label>UID <span>选填</span><input name="uid" inputMode="numeric" pattern="[0-9]*" maxLength={24} defaultValue={role?.uid} /></label>
              <label>区服 <span>选填</span><input name="server" maxLength={40} defaultValue={role?.server} /></label>
              <label>等级 <span>选填</span><input name="level" type="number" min={0} max={9999} defaultValue={role?.level ?? ""} /></label>
            </div>
            <div className="field-grid">
              <label>当前体力 <span>选填</span><input name="current" type="number" min={0} max={999999} defaultValue={stamina?.current} /></label>
              <label>体力上限 <span>选填</span><input name="maximum" type="number" min={1} max={999999} defaultValue={stamina?.maximum} /></label>
            </div>
          </>}
        </fieldset>
        {error && <p className="notice error" role="alert">{error}</p>}
        <div className="dialog-actions">
          <button className="button secondary" type="button" onClick={close} disabled={busy}>取消</button>
          <button className="button primary" type="submit" disabled={busy}>
            {busy ? (mode === "miyoushe" ? "正在读取角色…" : "正在保存…") : mode === "miyoushe" ? "读取并保存" : "保存账号"}
          </button>
        </div>
      </form>}
    </Modal>
  );
}

export function NoAccounts({ add }: { add: () => void }) {
  return <EmptyState icon={<UserRound size={29} />} title="还没有账号"
    detail="连接米游社读取角色，或手动添加其他游戏账号。"
    action={<button className="button primary" onClick={add}><Plus size={17} />添加第一个账号</button>} />;
}
