import { invoke } from "@tauri-apps/api/core";
import { RefreshCw, Search } from "lucide-react";
import { useEffect, useState } from "react";
import { Modal } from "./Accounts";
import { dateTime, errorMessage, games, type Account } from "./model";

interface LeveledItem {
  name: string;
  level: number | null;
}

interface CharacterDetails {
  weapon: LeveledItem | null;
  artifacts: LeveledItem[];
  skills: LeveledItem[];
}

interface Character {
  key: string;
  id: number;
  name: string;
  level: number | null;
  rank: number | null;
  details: CharacterDetails | null;
}

interface Roster {
  fetchedAt: number;
  characters: Character[];
}

function levelLabel(level: number | null) {
  return level === null ? "等级未知" : `Lv. ${level}`;
}

function Levels({ items, empty }: { items: LeveledItem[]; empty: string }) {
  return items.length ? <ul className="character-levels">
    {items.map(({ name, level }, index) => <li key={`${name}-${index}`}>
      <span>{name}</span><span className="muted">{levelLabel(level)}</span>
    </li>)}
  </ul> : <p className="small muted">{empty}</p>;
}

function CharacterPanel({ accountId, character, starrail }: {
  accountId: string; character: Character; starrail: boolean;
}) {
  const [details, setDetails] = useState(character.details);
  const [loading, setLoading] = useState(starrail);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const characterId = character.id;

  useEffect(() => {
    if (!starrail) return;
    let active = true;
    void invoke<CharacterDetails>("get_starrail_character", { id: accountId, characterId })
      .then(result => { if (active) setDetails(result); })
      .catch(reason => { if (active) setError(errorMessage(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [accountId, characterId, starrail, revision]);

  const rankLabel = starrail ? "星魂" : "命座";
  const weaponLabel = starrail ? "光锥" : "武器";
  const skillsLabel = starrail ? "行迹" : "天赋";
  return <section className="character-panel" aria-label={character.name}>
    <header>
      <h3>{character.name}</h3>
      <p className="small muted">{levelLabel(character.level)} · {rankLabel} {character.rank ?? "未知"}</p>
    </header>
    {loading && <p className="small muted" role="status">正在读取光锥和行迹…</p>}
    {error && <p className="notice error" role="alert">{error}<button type="button" className="text-button" onClick={() => {
      setError("");
      setLoading(true);
      setRevision(value => value + 1);
    }}>重试</button></p>}
    {details && <div className="character-details">
      <section>
        <h4>{weaponLabel}</h4>
        <Levels items={details.weapon ? [details.weapon] : []} empty={`暂无${weaponLabel}数据。`} />
      </section>
      {!starrail && <section>
        <h4>圣遗物</h4>
        <Levels items={details.artifacts} empty="暂无圣遗物数据。" />
      </section>}
      <section>
        <h4>{skillsLabel}</h4>
        <Levels items={details.skills} empty={`暂无${skillsLabel}数据。`} />
      </section>
    </div>}
  </section>;
}

export function CharactersDialog({ account, close }: { account: Account; close: () => void }) {
  const [result, setResult] = useState<{ roster: Roster; revision: number }>();
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [revision, setRevision] = useState(0);
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<string>();
  const id = account.id;
  const role = account.role;
  const starrail = role.game === "starrail";

  useEffect(() => {
    let active = true;
    void invoke<Roster>("get_characters", { id })
      .then(roster => { if (active) setResult({ roster, revision }); })
      .catch(reason => { if (active) setError(errorMessage(reason)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [id, revision]);

  const roster = result?.roster;
  const keyword = query.trim().toLowerCase();
  const characters = roster?.characters.filter(character => character.name.toLowerCase().includes(keyword)) ?? [];
  const character = characters.find(entry => entry.key === selected);

  return <Modal className="characters-dialog" title={`角色 · ${role.name}`} busy={false} close={close}>
    <div className="characters-toolbar">
      <div>
        <p className="small muted">{games[role.game].name} · {[role.serverName, `UID ${role.uid}`].filter(Boolean).join(" · ")}</p>
        {roster && <p className="small muted">{roster.characters.length} 名角色 · {dateTime.format(roster.fetchedAt * 1000)} 更新</p>}
      </div>
      <button type="button" className="button secondary" disabled={loading} onClick={() => {
        setLoading(true);
        setError("");
        setRevision(value => value + 1);
      }}><RefreshCw size={15} className={loading ? "spin" : undefined} />{loading ? "刷新中…" : "刷新角色"}</button>
    </div>
    {loading && <p className="small muted" role="status">正在读取当前账号的角色…</p>}
    {error && <p className="notice error" role="alert">{error}</p>}
    {roster && roster.characters.length === 0 && <p className="notice">该账号暂无角色。</p>}
    {roster && roster.characters.length > 0 && <div className="characters-layout" aria-busy={loading}>
      <div className="character-picker">
        <label className="character-search">
          <Search size={15} aria-hidden="true" />
          <input type="search" aria-label="搜索角色" placeholder="搜索角色" value={query}
            onChange={event => setQuery(event.target.value)} />
        </label>
        <ul className="character-list" aria-label="已拥有角色">
          {characters.map(entry => <li key={entry.key}>
            <button type="button" aria-pressed={entry.key === character?.key} onClick={() => setSelected(entry.key)}>
              <strong>{entry.name}</strong>
              <span className="small muted">{levelLabel(entry.level)} · {starrail ? "星魂" : "命座"} {entry.rank ?? "未知"}</span>
            </button>
          </li>)}
        </ul>
        {characters.length === 0 && <p className="small muted" role="status">没有匹配的角色。</p>}
      </div>
      {character ? <CharacterPanel key={`${result?.revision}-${character.key}`}
        accountId={id} character={character} starrail={starrail} /> : <div className="character-panel character-placeholder">
        <p className="small muted">选择角色查看{starrail ? "光锥和行迹" : "装备和天赋"}。</p>
      </div>}
    </div>}
  </Modal>;
}
