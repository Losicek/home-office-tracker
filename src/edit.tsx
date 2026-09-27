import { useEffect, useState } from "react";
import { api, Project, SessionRow } from "./api";
import { Dict, useT } from "./i18n";
import { ProjectDot } from "./projects";

/** ms → hodnota pro <input type="datetime-local"> v místním čase. */
function toInput(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

function fromInput(value: string): number {
  return new Date(value).getTime();
}

function errorText(t: Dict, e: unknown): string {
  const code = String(e);
  if (code.includes("overlap")) return t.errOverlap;
  if (code.includes("invalid-range")) return t.errInvalidRange;
  if (code.includes("in-future")) return t.errInFuture;
  if (code.includes("running")) return t.errRunning;
  return code;
}

/**
 * Úprava existující akce (`session`), nebo nový ruční záznam (`session`
 * chybí, `day` určuje předvyplněný den). U běžící akce jde měnit jen
 * poznámka.
 */
export function EditSessionDialog({
  session,
  day,
  running,
  projects,
  onClose,
}: {
  session?: SessionRow;
  day?: Date;
  running?: boolean;
  projects: Project[];
  onClose: () => void;
}) {
  const t = useT();
  const base = day ?? new Date();
  const defaultStart = new Date(base.getFullYear(), base.getMonth(), base.getDate(), 9).getTime();
  const [start, setStart] = useState(toInput(session?.startedAt ?? defaultStart));
  const [end, setEnd] = useState(
    toInput(session?.endedAt ?? Math.min(defaultStart + 60 * 60_000, Date.now())),
  );
  const [project, setProject] = useState<number | null>(session?.projectId ?? null);
  const [note, setNote] = useState(session?.note ?? "");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  // Aktivní projekty + archivovaný, pokud ho akce už má.
  const options = projects.filter((p) => !p.archived || p.id === session?.projectId);
  const current = projects.find((p) => p.id === project);

  const save = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    const input = {
      startedAt: fromInput(start),
      endedAt: fromInput(end),
      project,
      note: note.trim() || null,
    };
    try {
      if (session) await api.updateSession(session.id, input);
      else await api.addSession(input);
      onClose();
    } catch (err) {
      setError(errorText(t, err));
    }
  };

  const remove = async () => {
    if (!session || !window.confirm(t.deleteConfirm)) return;
    try {
      await api.deleteSession(session.id);
      onClose();
    } catch (err) {
      setError(errorText(t, err));
    }
  };

  return (
    <div className="modal-backdrop" onClick={onClose}>
      <form className="card modal settings" onClick={(e) => e.stopPropagation()} onSubmit={save}>
        <h2>{session ? t.editEntry : t.newEntry}</h2>
        {running && <p className="muted">{t.runningOnlyNote}</p>}
        <div className="modal-row">
          <label>
            {t.start_}
            <input
              type="datetime-local"
              value={start}
              onChange={(e) => setStart(e.target.value)}
              disabled={running}
              required
            />
          </label>
          <label>
            {t.end_}
            <input
              type="datetime-local"
              value={end}
              onChange={(e) => setEnd(e.target.value)}
              disabled={running}
              required
            />
          </label>
        </div>
        <label>
          {t.project}
          <div className="project-select">
            <ProjectDot color={current?.color} />
            <select
              value={project ?? ""}
              onChange={(e) => setProject(e.target.value === "" ? null : Number(e.target.value))}
              disabled={running}
            >
              <option value="">{t.noProject}</option>
              {options.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.name}
                </option>
              ))}
            </select>
          </div>
        </label>
        <label>
          {t.noteLabel}
          <textarea
            value={note}
            onChange={(e) => setNote(e.target.value)}
            placeholder={t.notePlaceholder}
            maxLength={500}
            rows={3}
          />
        </label>
        {error && <div className="error">{error}</div>}
        <div className="actions modal-actions">
          {session && !running && (
            <button type="button" className="btn danger" onClick={remove}>
              {t.deleteEntry}
            </button>
          )}
          <span className="spacer" />
          <button type="button" className="btn" onClick={onClose}>
            {t.cancel}
          </button>
          <button type="submit" className="btn primary">
            {t.save}
          </button>
        </div>
      </form>
    </div>
  );
}
