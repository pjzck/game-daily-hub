import { invoke } from "@tauri-apps/api/core";
import { Monitor, Play, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { EmptyState } from "./Accounts";
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
  const [starting, setStarting] = useState<number>();
  const [updated, setUpdated] = useState<number>();
  const [error, setError] = useState("");

  async function readInstances() {
    setRows(await invoke<Instance[]>("list_mumu_instances"));
    setUpdated(Date.now());
  }

  async function refresh() {
    setBusy(true);
    setError("");
    try {
      await readInstances();
    } catch (reason) {
      setError(errorMessage(reason) + (rows.length > 0 ? " 已保留上次列表。" : ""));
    } finally {
      setBusy(false);
    }
  }

  async function start(row: Instance) {
    setStarting(row.index);
    setError("");
    try {
      setRows(await invoke<Instance[]>("start_mumu_instance", { index: row.index, name: row.name }));
      setUpdated(Date.now());
    } catch (reason) {
      setError(errorMessage(reason));
      try {
        await readInstances();
      } catch {
        setError(message => message + " 状态读取失败，已保留上次列表。");
      }
    } finally {
      setStarting(undefined);
    }
  }

  useEffect(() => { void refresh(); }, []);

  return <>
    <header className="page-header">
      <div><h1>MuMu 实例</h1><p className="muted">{updated ? "上次读取 " + dateTime.format(updated) : "本机模拟器"}</p></div>
      <button className="button primary" onClick={refresh} disabled={busy || starting !== undefined}>
        <RefreshCw size={17} className={busy ? "spin" : ""} />{busy ? "读取中…" : "刷新实例"}
      </button>
    </header>
    {error && <div className="notice error" role="alert">{error}</div>}
    {rows.length > 0 ? <div className="panel table-panel">
      <div className="panel-title">
        <h2>{rows.length} 个实例</h2><span className="small muted">{rows.filter(row => row.status === "running").length} 个运行中</span>
      </div>
      <table>
        <thead><tr><th>实例</th><th>Android</th><th>ADB 地址</th><th>状态</th><th>操作</th></tr></thead>
        <tbody>{rows.map(row => {
          const [label, tone] = statuses[starting === row.index ? "starting" : row.status];
          return <tr key={row.index}>
            <td><div className="table-device"><span className="device-mark"><Monitor size={18} /></span>
              <div><strong>{row.name}</strong><div className="small muted">实例 {row.index}</div></div></div></td>
            <td>{row.androidVersion ?? "—"}</td>
            <td className="mono">{row.adb ?? "—"}</td>
            <td><span className={"status-pill " + tone} role="status">{label}</span></td>
            <td>{row.canStart && <button type="button" className="button secondary device-start"
              disabled={busy || starting !== undefined} aria-label={`启动「${row.name}」`}
              onClick={() => void start(row)}>
              {starting === row.index ? <RefreshCw size={15} className="spin" /> : <Play size={15} />}
              启动
            </button>}</td>
          </tr>;
        })}</tbody>
      </table>
    </div> : <EmptyState icon={<Monitor size={29} />}
      title={busy ? "正在读取实例" : error ? "暂时无法读取实例" : "没有找到实例"} />}
  </>;
}
