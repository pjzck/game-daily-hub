import { invoke } from "@tauri-apps/api/core";
import { Monitor, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { EmptyState } from "./Accounts";
import { AutomationTools, LauncherPath, LauncherPathActions, useLaunchers } from "./AutomationTools";
import { dateTime, errorMessage } from "./model";

type Status = "running" | "starting" | "stopped" | "unknown" | "error";
interface Instance {
  index: number;
  name: string;
  androidVersion: string | null;
  status: Status;
  adb: string | null;
  canStart: boolean;
}
const statuses: Record<Status, [string, string]> = {
  running: ["运行中", "good"],
  starting: ["启动中", "warning"],
  stopped: ["已停止", "neutral"],
  unknown: ["未知", "neutral"],
  error: ["异常", "warning"],
};

export function Devices() {
  const [rows, setRows] = useState<Instance[]>([]);
  const [busy, setBusy] = useState(true);
  const [starting, setStarting] = useState<ReadonlySet<number>>(() => new Set());
  const [startErrors, setStartErrors] = useState<Record<number, string>>({});
  const [updated, setUpdated] = useState<number>();
  const [error, setError] = useState("");
  const launchers = useLaunchers();
  const manager = launchers.rows.find(row => row.tool === "mumu");

  async function readInstances() {
    setRows(await invoke<Instance[]>("list_mumu_instances"));
    setUpdated(Date.now());
  }

  async function refresh(managerChanged = false) {
    setBusy(true);
    setError("");
    if (managerChanged) {
      setRows([]);
      setUpdated(undefined);
      setStartErrors({});
    }
    try {
      await readInstances();
      setStartErrors({});
    } catch (reason) {
      setError(errorMessage(reason) + (!managerChanged && rows.length > 0 ? " 已保留上次列表。" : ""));
    } finally {
      setBusy(false);
    }
  }

  async function configureManager(action: "choose" | "reset") {
    if (await launchers.run("mumu", action)) await refresh(true);
  }

  function updateInstance(instance: Instance) {
    setRows(current => current.map(row => row.index === instance.index ? instance : row));
    setUpdated(Date.now());
  }

  async function start(row: Instance) {
    const { index, name } = row;
    setStarting(current => new Set(current).add(index));
    setStartErrors(current => ({ ...current, [index]: "" }));
    try {
      updateInstance(await invoke<Instance>("start_mumu_instance", { index, name }));
    } catch (reason) {
      let message = errorMessage(reason);
      try {
        const latest = await invoke<Instance[]>("list_mumu_instances");
        const instance = latest.find(entry => entry.index === index && entry.name === name);
        if (instance) updateInstance(instance);
      } catch {
        message += " 暂时无法读取最新状态。";
      }
      setStartErrors(current => ({ ...current, [index]: message }));
    } finally {
      setStarting(current => {
        const next = new Set(current);
        next.delete(index);
        return next;
      });
    }
  }

  useEffect(() => { void refresh(); }, []);

  return <>
    <header className="page-header">
      <div><h1>MuMu 实例</h1><p className="muted">{updated ? "上次读取 " + dateTime.format(updated) : "本机模拟器"}</p></div>
      <div className="device-page-actions">
        <LauncherPathActions row={manager} disabled={busy || launchers.loading || !!launchers.pending.mumu || starting.size > 0}
          onChoose={() => void configureManager("choose")} onReset={() => void configureManager("reset")} />
        <button className="button primary" onClick={() => { void refresh(); void launchers.refresh(); }}
          disabled={busy || launchers.loading || Object.values(launchers.pending).some(Boolean) || starting.size > 0}>
          <RefreshCw size={17} className={busy ? "spin" : ""} />{busy ? "读取中…" : "自动检测实例"}
        </button>
      </div>
    </header>
    <div className="mumu-launcher-path"><LauncherPath row={manager} /></div>
    {launchers.issues.mumu && <div className="notice error" role="alert">{launchers.issues.mumu}</div>}
    {error && <div className="notice error" role="alert">{error}</div>}
    {rows.length > 0 ? <div className="panel table-panel">
      <div className="panel-title">
        <h2>{rows.length} 个实例</h2><span className="small muted">{rows.filter(row => row.status === "running").length} 个运行中</span>
      </div>
      <table>
        <thead><tr><th>实例</th><th>Android</th><th>ADB 地址</th><th>状态</th><th>操作</th></tr></thead>
        <tbody>{rows.map(row => {
          const isStarting = starting.has(row.index);
          const [label, tone] = statuses[isStarting ? "starting" : row.status];
          const startError = startErrors[row.index];
          return <tr key={row.index}>
            <td><div className="table-device"><span className="device-mark"><Monitor size={18} /></span>
              <div><strong>{row.name}</strong><div className="small muted">实例 {row.index}</div></div></div></td>
            <td>{row.androidVersion ?? "—"}</td>
            <td className="mono">{row.adb ?? "—"}</td>
            <td>
              <span className={"status-pill " + tone} role="status">{label}</span>
              {startError && <p className="small device-start-error" role="alert">{startError}</p>}
            </td>
            <td>{row.canStart && <button type="button" className="button secondary device-start"
              disabled={busy || !!launchers.pending.mumu || isStarting} aria-label={`启动「${row.name}」`}
              onClick={() => void start(row)}>
              {isStarting ? <RefreshCw size={15} className="spin" /> : <Play size={15} />}
              启动
            </button>}</td>
          </tr>;
        })}</tbody>
      </table>
    </div> : <EmptyState icon={<Monitor size={29} />}
      title={busy ? "正在读取实例" : error ? "暂时无法读取实例" : "没有找到实例"} />}
    <AutomationTools data={launchers} />
  </>;
}
