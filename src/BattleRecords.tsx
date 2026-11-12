import { invoke } from "@tauri-apps/api/core";
import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { Modal } from "./Accounts";
import { dateTime, errorMessage, games, type Account } from "./model";

const modes = {
  theater: "幻想真境剧诗",
  stygian_onslaught: "幽境危战",
  anomaly_arbitration: "异相仲裁",
} as const;

type ScheduleDate = number | string;
interface Season {
  name: string | null;
  startsOn: ScheduleDate | null;
  endsOn: ScheduleDate | null;
  groups: {
    name: string | null;
    hasRecord: boolean | null;
    metrics: { label: string; value: string }[];
  }[];
}
type ModeRecord = { mode: keyof typeof modes } & (
  { status: "records"; seasons: Season[] } |
  { status: "locked" } |
  { status: "error"; message: string }
);
interface BattleRecords {
  fetchedAt: number;
  modes: ModeRecord[];
}

const recordDay = new Intl.DateTimeFormat("zh-CN", {
  year: "numeric", month: "2-digit", day: "2-digit", timeZone: "Asia/Shanghai",
});

function seasonDates(season: Season) {
  const format = (date: ScheduleDate | null) => typeof date === "number" ? recordDay.format(date * 1000) : date?.replaceAll("-", "/");
  const from = format(season.startsOn);
  const to = format(season.endsOn);
  return from && to ? `${from} – ${to}` : from ?? to;
}

function SeasonRecord({ season, index }: { season: Season; index: number }) {
  const dates = seasonDates(season);
  return <details className="battle-season" open={index === 0}>
    <summary>
      <strong>{season.name || dates || `记录 ${index + 1}`}</strong>
      {season.name && dates && <span className="small muted">{dates}</span>}
    </summary>
    {season.groups.length === 0 && <p className="small muted">米游社未提供该期成绩。</p>}
    {season.groups.map(group => <div className="battle-group" key={group.name ?? "main"}>
      {group.name && <h4>{group.name}</h4>}
      {group.hasRecord === false ? <p className="small muted">本期暂无挑战记录。</p> : group.hasRecord && group.metrics.length ? (
        <dl className="battle-metrics">
          {group.metrics.map(metric => <div key={metric.label}><dt>{metric.label}</dt><dd>{metric.value}</dd></div>)}
        </dl>
      ) : <p className="small muted">米游社未提供成绩摘要。</p>}
    </div>)}
  </details>;
}

function ModeSection({ record }: { record: ModeRecord }) {
  return <section className="battle-mode">
    <h3>{modes[record.mode]}</h3>
    {record.status === "locked" && <p className="small muted">该角色尚未解锁此玩法。</p>}
    {record.status === "error" && <p className="notice error" role="alert">{record.message}</p>}
    {record.status === "records" && (record.seasons.length ? record.seasons.map((season, index) => (
      <SeasonRecord key={index} season={season} index={index} />
    )) : <p className="small muted">暂无期次记录。</p>)}
  </section>;
}

export function BattleRecordsDialog({ account, close }: { account: Account; close: () => void }) {
  const [records, setRecords] = useState<BattleRecords | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const id = account.id;

  useEffect(() => {
    let active = true;
    void invoke<BattleRecords>("get_battle_records", { id })
      .then(result => { if (active) setRecords(result); })
      .catch(reason => { if (active) setError(errorMessage(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [id, revision]);

  return <Modal title={`战绩 · ${account.role.name}`} busy={false} close={close}>
    <div className="battle-toolbar">
      <div>
        <p className="small muted">{games[account.role.game].name} · {account.role.serverName} · UID {account.role.uid}</p>
        {records && <p className="small muted">查询于 {dateTime.format(records.fetchedAt * 1000)}</p>}
      </div>
      <button type="button" className="button secondary" disabled={loading} onClick={() => {
        setLoading(true);
        setError("");
        setRevision(value => value + 1);
      }}><RefreshCw size={15} className={loading ? "spin" : undefined} />{loading ? "查询中…" : "刷新战绩"}</button>
    </div>
    {loading && <p className="small muted" role="status">正在从米游社读取战绩…</p>}
    {error && <p className="notice error" role="alert">{error}</p>}
    <div className="battle-records" aria-busy={loading}>
      {records?.modes.map(record => <ModeSection key={record.mode} record={record} />)}
    </div>
  </Modal>;
}
