import { useCallback, useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, Project, PROJECT_COLORS } from "./api";
import { duration } from "./format";
import { useT } from "./i18n";

/** Seznam projektů, obnovuje se při každé změně v jádru. */
export function useProjects(): [Project[], () => void] {
  const [projects, setProjects] = useState<Project[]>([]);
  const refresh = useCallback(() => {
    api.projects().then(setProjects).catch(() => {});
  }, []);
  useEffect(() => {
    refresh();
    const unlisten = listen("tracker-changed", refresh);
    return () => {
      unlisten.then((u) => u());
    };
  }, [refresh]);
  return [projects, refresh];
}

export function ProjectDot({ color }: { color: string | null | undefined }) {
  return <span className="project-dot" style={{ background: color ?? "transparent" }} />;
}

function ColorPicker({ value, onChange }: { value: string; onChange: (c: string) => void }) {
  return (
    <div className="color-picker">
      {PROJECT_COLORS.map((c) => (
        <button
          key={c}
          type="button"
          className={c === value ? "swatch active" : "swatch"}
          style={{ background: c }}
          onClick={() => onChange(c)}
          aria-label={c}
        />
      ))}
    </div>
  );
}

/** Výběr projektu: „Bez projektu“ + nearchivované projekty. */
export function ProjectSelect({
  projects,
  value,
  onChange,
}: {
  projects: Project[];
  value: number | null;
  onChange: (id: number | null) => void;
}) {
  const t = useT();
  const active = projects.filter((p) => !p.archived);
  const current = projects.find((p) => p.id === value);
  return (
    <div className="project-select">
      <ProjectDot color={current?.color} />
      <select
        value={value ?? ""}
        onChange={(e) => onChange(e.target.value === "" ? null : Number(e.target.value))}
      >
        <option value="">{t.noProject}</option>
        {active.map((p) => (
          <option key={p.id} value={p.id}>
            {p.name}
          </option>
        ))}
      </select>
    </div>
  );
}

/** Formulář pro nový / upravovaný projekt (název + barva). */
function ProjectForm({
  initialName = "",
  initialColor = PROJECT_COLORS[0],
  submitLabel,
  onSubmit,
  onCancel,
}: {
  initialName?: string;
  initialColor?: string;
  submitLabel: string;
  onSubmit: (name: string, color: string) => Promise<void>;
  onCancel?: () => void;
}) {
  const t = useT();
  const [name, setName] = useState(initialName);
  const [color, setColor] = useState(initialColor);
  const [error, setError] = useState(false);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError(true);
      return;
    }
    onSubmit(name, color).then(() => {
      setName("");
      setError(false);
    });
  };

  return (
    <form className="project-form" onSubmit={submit}>
      <input
        value={name}
        onChange={(e) => setName(e.target.value)}
        placeholder={t.projectNamePlaceholder}
        maxLength={80}
        autoFocus={!!onCancel}
      />
      <ColorPicker value={color} onChange={setColor} />
      <div className="actions">
        <button className="btn primary" type="submit">
          {submitLabel}
        </button>
        {onCancel && (
          <button className="btn" type="button" onClick={onCancel}>
            {t.cancel}
          </button>
        )}
      </div>
      {error && <div className="error">{t.emptyName}</div>}
    </form>
  );
}

/** Rychlé založení projektu přímo na obrazovce Dnes. */
export function QuickCreateProject({ onCreated }: { onCreated: (id: number) => void }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  if (!open) {
    return (
      <button className="btn link-btn" type="button" onClick={() => setOpen(true)}>
        + {t.newProject}
      </button>
    );
  }
  return (
    <div className="card inline-card">
      <ProjectForm
        submitLabel={t.create}
        onSubmit={async (name, color) => {
          const id = await api.createProject(name, color);
          setOpen(false);
          onCreated(id);
        }}
        onCancel={() => setOpen(false)}
      />
    </div>
  );
}

export function ProjectsView() {
  const t = useT();
  const [projects, refresh] = useProjects();
  const [editing, setEditing] = useState<number | null>(null);
  const active = projects.filter((p) => !p.archived);
  const archived = projects.filter((p) => p.archived);

  const save = (p: Project, changes: Partial<Project>) =>
    api.updateProject({ ...p, ...changes }).then(() => {
      setEditing(null);
      refresh();
    });

  const row = (p: Project) =>
    editing === p.id ? (
      <li key={p.id} className="project-row editing">
        <ProjectForm
          initialName={p.name}
          initialColor={p.color}
          submitLabel={t.save}
          onSubmit={(name, color) => save(p, { name, color })}
          onCancel={() => setEditing(null)}
        />
      </li>
    ) : (
      <li key={p.id} className="project-row">
        <ProjectDot color={p.color} />
        <span className="project-name">{p.name}</span>
        <span className="muted num">
          {t.totalTime} {duration(p.totalMs)}
        </span>
        {!p.archived && (
          <button className="btn small" onClick={() => setEditing(p.id)}>
            {t.edit}
          </button>
        )}
        <button className="btn small" onClick={() => save(p, { archived: !p.archived })}>
          {p.archived ? t.unarchive : t.archive}
        </button>
      </li>
    );

  return (
    <div className="projects-page">
      <section className="card">
        <h2>{t.newProject}</h2>
        <ProjectForm
          submitLabel={t.create}
          onSubmit={async (name, color) => {
            await api.createProject(name, color);
            refresh();
          }}
        />
      </section>

      <section className="card">
        <h2>{t.tabProjects}</h2>
        {active.length === 0 ? (
          <p className="muted">{t.noProjects}</p>
        ) : (
          <ul className="project-list">{active.map(row)}</ul>
        )}
      </section>

      {archived.length > 0 && (
        <section className="card">
          <h2>{t.archived}</h2>
          <p className="muted">{t.projectsHelp}</p>
          <ul className="project-list">{archived.map(row)}</ul>
        </section>
      )}
    </div>
  );
}
