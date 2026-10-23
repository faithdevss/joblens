import type { Role } from "../bindings/Role";
import { dateFlag } from "../lib/dates";
import { newId } from "../lib/profile";
import { Icon } from "./Icon";

type RoleEditorProps = {
  role: Role;
  allRoles: Role[];
  onChange: (role: Role) => void;
  onRemove: () => void;
};

export function RoleEditor({ role, allRoles, onChange, onRemove }: RoleEditorProps) {
  const flag = dateFlag(role, allRoles);
  const name = role.title || "untitled role";

  const setAchievement = (id: string, text: string) =>
    onChange({
      ...role,
      achievements: role.achievements.map((a) => (a.id === id ? { ...a, text } : a)),
    });

  return (
    <div className="card role">
      <div className="role-head">
        <label className="field">
          <span className="field-label">Role</span>
          <input
            value={role.title}
            onChange={(e) => onChange({ ...role, title: e.target.value })}
          />
        </label>
        <label className="field">
          <span className="field-label">Company</span>
          <input
            value={role.company}
            onChange={(e) => onChange({ ...role, company: e.target.value })}
          />
        </label>
        <label className="field">
          <span className="field-label">Dates</span>
          <input
            value={role.dates}
            placeholder="Mar 2021 – Present"
            data-flagged={flag ? true : undefined}
            onChange={(e) => onChange({ ...role, dates: e.target.value, datesConfirmed: false })}
          />
        </label>
        <button
          type="button"
          className="icon-button"
          aria-label={`Remove ${name}`}
          onClick={onRemove}
        >
          <Icon name="delete" />
        </button>
      </div>

      {flag && (
        <div className="flag" role="note">
          <Icon name="error" />
          <span>{flag}</span>
          <button
            type="button"
            className="btn btn-ghost"
            onClick={() => onChange({ ...role, datesConfirmed: true })}
          >
            Looks right
          </button>
        </div>
      )}

      <div className="field">
        <span className="field-label">Achievements</span>
        {role.achievements.map((a, i) => (
          <div className="achievement" key={a.id}>
            <textarea
              rows={2}
              value={a.text}
              aria-label={`Achievement ${i + 1} at ${role.company || name}`}
              placeholder="What you did and the result"
              onChange={(e) => setAchievement(a.id, e.target.value)}
            />
            <button
              type="button"
              className="icon-button"
              aria-label={`Remove achievement ${i + 1} at ${role.company || name}`}
              onClick={() =>
                onChange({ ...role, achievements: role.achievements.filter((x) => x.id !== a.id) })
              }
            >
              <Icon name="delete" />
            </button>
          </div>
        ))}
        <button
          type="button"
          className="btn btn-link"
          onClick={() =>
            onChange({
              ...role,
              achievements: [...role.achievements, { id: newId("a"), text: "", lines: [] }],
            })
          }
        >
          <Icon name="add" />
          Add achievement
        </button>
      </div>
    </div>
  );
}

export function newRole(): Role {
  return {
    id: newId("r"),
    title: "",
    company: "",
    dates: "",
    datesConfirmed: false,
    achievements: [{ id: newId("a"), text: "", lines: [] }],
    lines: [],
  };
}
