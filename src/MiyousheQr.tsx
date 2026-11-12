import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { errorMessage, type Account } from "./model";

type LoginQr = { id: string; width: number; modules: boolean[]; expiresInSeconds: number };
type LoginStatus = "waiting" | "scanned" | "confirmed" | "expired" | "canceled";

const statusText: Record<LoginStatus, string> = {
  waiting: "请用米游社 App 扫码。",
  scanned: "已扫码，请在手机上确认登录。",
  confirmed: "已确认，正在读取角色和实时便笺…",
  expired: "二维码已过期，请重新生成。",
  canceled: "扫码会话已结束，请重新生成。",
};

function QrCanvas({ qr }: { qr: LoginQr }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const cell = 6;
  const border = 4;
  const size = (qr.width + border * 2) * cell;
  useEffect(() => {
    const context = canvas.current?.getContext("2d");
    if (!context) return;
    context.fillStyle = "#fff";
    context.fillRect(0, 0, size, size);
    context.fillStyle = "#000";
    qr.modules.forEach((dark, index) => {
      if (!dark) return;
      const x = (index % qr.width + border) * cell;
      const y = (Math.floor(index / qr.width) + border) * cell;
      context.fillRect(x, y, cell, cell);
    });
  }, [qr, size]);
  return <canvas ref={canvas} className="qr-image" width={size} height={size} role="img" aria-label="米游社登录二维码" />;
}

export function MiyousheQr({ existingId, setBusy, saved }: {
  existingId: string | null;
  setBusy: (busy: boolean) => void;
  saved: (accounts: Account[]) => void;
}) {
  const [qr, setQr] = useState<LoginQr | null>(null);
  const [status, setStatus] = useState<LoginStatus>("waiting");
  const [retry, setRetry] = useState(0);
  const [busy, setLocalBusy] = useState(false);
  const [error, setError] = useState("");
  const savedRef = useRef(saved);

  useEffect(() => { savedRef.current = saved; }, [saved]);

  useEffect(() => {
    if (!qr) return;
    return () => { void invoke("cancel_miyoushe_login", { id: qr.id }).catch(() => {}); };
  }, [qr]);

  useEffect(() => {
    if (!qr) return;
    const id = qr.id;
    let active = true;
    let timer: number;

    async function check() {
      try {
        const next = await invoke<LoginStatus>("poll_miyoushe_login", { id });
        if (!active) return;
        setStatus(next);
        if (next === "waiting" || next === "scanned") {
          timer = window.setTimeout(check, 1600);
        } else if (next === "confirmed") {
          setBusy(true);
          setLocalBusy(true);
          try {
            const accounts = await invoke<Account[]>("finish_miyoushe_login", { id });
            if (active) savedRef.current(accounts);
          } finally {
            if (active) {
              setBusy(false);
              setLocalBusy(false);
            }
          }
        }
      } catch (reason) {
        if (active) setError(errorMessage(reason));
      }
    }

    timer = window.setTimeout(check, 800);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [qr, retry, setBusy]);

  async function generate() {
    setQr(null);
    setStatus("waiting");
    setError("");
    setBusy(true);
    setLocalBusy(true);
    try {
      setQr(await invoke<LoginQr>("start_miyoushe_login", { existingId }));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
      setLocalBusy(false);
    }
  }

  const showCode = qr && (status === "waiting" || status === "scanned");
  return <div className="qr-login" aria-busy={busy}>
    <p className="form-description">读取该米游社账号下绑定的原神、崩铁国服角色和实时便笺。登录凭据由 Windows 加密保存。</p>
    {showCode && <QrCanvas qr={qr} />}
    {qr && !error && <p className="qr-status" role="status">
      {statusText[status]}{status === "waiting" && `二维码约 ${Math.ceil(qr.expiresInSeconds / 60)} 分钟后过期。`}
    </p>}
    {error && <p className="notice error" role="alert">{error}</p>}
    <div className="dialog-actions">
      {qr && error && <button className="button secondary" type="button" disabled={busy} onClick={() => {
        setError("");
        setRetry((value) => value + 1);
      }}>{status === "confirmed" ? "重试读取" : "继续检查"}</button>}
      {status !== "confirmed" || error ? <button className="button primary" type="button" disabled={busy} onClick={generate}>
        {qr ? "重新生成二维码" : busy ? "正在生成…" : "生成二维码"}
      </button> : null}
    </div>
  </div>;
}
