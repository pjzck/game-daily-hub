import { invoke } from "@tauri-apps/api/core";
import { CircleAlert, Link2, Pencil, Plus, Trash2, UserRound, X } from "lucide-react";
import { useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import { dateTime, errorMessage, games, type Account, type Game, type Meter } from "./model";

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

export function AccountCard({ account, onEdit, onRemove }: {
  account: Account;
  onEdit: () => void;
  onRemove: () => void;
}) {
  const { role, notes, issue } = account;
  const game = games[role.game];
  const manual = account.source === "manual";
  const status = manual ? "手动记录" : issue ? (notes ? "数据待更新" : "待处理") : "已同步";
  const details = [role.serverName, role.uid && "UID " + role.uid, role.level !== null && "Lv. " + role.level].filter(Boolean);
  return (
    <article className="account-card">
      <div className="account-heading">
        <GameMark game={role.game} />
        <div className="account-identity">
          <p className="game-name">{game.name}</p>
          <h3 title={role.name}>{role.name}</h3>
        </div>
        <span className={"status-pill " + (issue ? "warning" : manual ? "neutral" : "good")}>{status}</span>
      </div>
      <p className="role-detail">{details.join(" · ") || "未填写区服与 UID"}</p>
      <div className="account-meters">
        <ResourceMeter label={game.stamina} meter={notes?.stamina ?? null} />
        {notes?.activity && <ResourceMeter label={game.activity} meter={notes.activity} />}
        {notes?.fullAt && notes.stamina && notes.stamina.current < notes.stamina.maximum ? (
          <p className="small muted">预计 {dateTime.format(notes.fullAt * 1000)} 回满</p>
        ) : null}
      </div>
      {issue && <p className="card-issue"><CircleAlert size={15} />{issue}</p>}
      <div className="account-footer">
        <div className="small muted">
          <div>{manual ? "本地记录" : "米游社"} · 资料 {dateTime.format(account.profileAt * 1000)}</div>
          {account.notesAt && !manual && <div>便笺 {dateTime.format(account.notesAt * 1000)}</div>}
        </div>
        <div className="icon-actions">
          <button className="icon-button" type="button" onClick={onEdit}
            title={manual ? "编辑账号" : "更新连接"} aria-label={manual ? "编辑账号" : "更新连接"}>
            {manual ? <Pencil size={17} /> : <Link2 size={17} />}
          </button>
          <button className="icon-button delete-button" type="button" onClick={onRemove}
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

export function AccountDialog({ account, close, saved }: {
  account: Account | null;
  close: () => void;
  saved: (accounts: Account[]) => void;
}) {
  const [mode, setMode] = useState<"miyoushe" | "manual">(account?.source ?? "miyoushe");
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
    <Modal title={account ? (mode === "manual" ? "编辑账号" : "更新米游社连接") : "添加账号"} busy={busy} close={close}>
      {!account && <div className="segments" aria-label="账号来源">
        <button type="button" aria-pressed={mode === "miyoushe"} onClick={() => { setMode("miyoushe"); setError(""); }} disabled={busy}><Link2 size={16} /> 连接米游社</button>
        <button type="button" aria-pressed={mode === "manual"} onClick={() => { setMode("manual"); setError(""); }} disabled={busy}><Pencil size={16} /> 手动录入</button>
      </div>}
      <form onSubmit={submit} key={mode} aria-busy={busy}>
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
      </form>
    </Modal>
  );
}

export function NoAccounts({ add }: { add: () => void }) {
  return <EmptyState icon={<UserRound size={29} />} title="还没有账号"
    detail="连接米游社读取角色，或手动添加其他游戏账号。"
    action={<button className="button primary" onClick={add}><Plus size={17} />添加第一个账号</button>} />;
}
