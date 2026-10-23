import type { ReactNode } from "react";
import type { Profile } from "../bindings/Profile";
import { Icon } from "../components/Icon";
import { ResumeDrop } from "../components/ResumeDrop";
import { newRole, RoleEditor } from "../components/RoleEditor";
import { SaveIndicator } from "../components/SaveIndicator";
import { SkillsInput } from "../components/SkillsInput";
import { newId } from "../lib/profile";
import type { AppData } from "../lib/useAppData";
import { ViewHeader } from "./ViewHeader";

type ResumeViewProps = {
  data: AppData;
};

const LANGUAGE_LEVELS = ["Native", "Fluent", "Professional", "Basic"];

type ListKey = "education" | "projects" | "languages" | "certifications";

export function ResumeView({ data }: ResumeViewProps) {
  const { profile, updateProfile } = data;
  const set = <K extends keyof Profile>(key: K, value: Profile[K]) =>
    updateProfile({ ...profile, [key]: value });
  const setContact = (key: keyof Profile["contact"], value: string) =>
    set("contact", { ...profile.contact, [key]: value });

  // Generic edit/remove for the simple list sections.
  const update = <K extends ListKey>(key: K, id: string, patch: Partial<Profile[K][number]>) =>
    set(
      key,
      (profile[key] as Profile[K][number][]).map((x) =>
        x.id === id ? { ...x, ...patch } : x,
      ) as Profile[K],
    );
  const remove = <K extends ListKey>(key: K, id: string) =>
    set(key, (profile[key] as Profile[K][number][]).filter((x) => x.id !== id) as Profile[K]);

  const words = profile.summary.trim() ? profile.summary.trim().split(/\s+/).length : 0;

  const sections: Array<{ id: string; label: string; count?: number }> = [
    { id: "contact", label: "Contact" },
    { id: "summary", label: "Summary" },
    { id: "experience", label: "Experience", count: profile.roles.length },
    { id: "skills", label: "Skills", count: profile.skills.length },
    { id: "education", label: "Education", count: profile.education.length },
    { id: "projects", label: "Projects", count: profile.projects.length },
    { id: "languages", label: "Languages", count: profile.languages.length },
    { id: "certs", label: "Certifications", count: profile.certifications.length },
  ];

  return (
    <section className="view">
      <ViewHeader
        title="Resume"
        subtitle="Matches, letters and interview prep only use what's in this profile."
        aside={<SaveIndicator status={data.profileSaveStatus} />}
      />
      <div className="view-body">
        <ResumeDrop
          resume={data.stored?.resume ?? null}
          profile={profile}
          onImport={data.importResume}
        />

        <nav className="section-nav" aria-label="Resume sections">
          {sections.map((s) => (
            <button
              key={s.id}
              type="button"
              onClick={() =>
                document.getElementById(`res-${s.id}`)?.scrollIntoView({ behavior: "smooth" })
              }
            >
              {s.label}
              {s.count !== undefined && <span className="nav-count">{s.count}</span>}
            </button>
          ))}
        </nav>

        <Section id="contact" icon="badge" title="Contact" sub="Shown at the top of every letter.">
          <div className="form-row form-row-2">
            <TextField
              label="Full name"
              value={profile.contact.name}
              onChange={(v) => setContact("name", v)}
            />
            <TextField
              label="Headline"
              value={profile.contact.headline}
              placeholder="e.g. Senior Frontend Engineer"
              onChange={(v) => setContact("headline", v)}
            />
          </div>
          <div className="form-row form-row-3">
            <TextField
              label="Email"
              value={profile.contact.email}
              onChange={(v) => setContact("email", v)}
            />
            <TextField
              label="Phone"
              value={profile.contact.phone}
              onChange={(v) => setContact("phone", v)}
            />
            <TextField
              label="Location"
              value={profile.contact.location}
              onChange={(v) => setContact("location", v)}
            />
          </div>
          <div className="form-row form-row-3">
            <TextField
              label="Website"
              value={profile.contact.website}
              placeholder="yoursite.com"
              onChange={(v) => setContact("website", v)}
            />
            <TextField
              label="GitHub"
              value={profile.contact.github}
              onChange={(v) => setContact("github", v)}
            />
            <TextField
              label="LinkedIn"
              value={profile.contact.linkedin}
              onChange={(v) => setContact("linkedin", v)}
            />
          </div>
        </Section>

        <Section
          id="summary"
          icon="notes"
          title="Summary"
          sub="Two or three sentences. Used to open cover letters."
          aside={
            <span className="section-meta">
              {words} words{words > 70 ? " · try to trim" : ""}
            </span>
          }
        >
          <textarea
            rows={4}
            aria-label="Summary"
            value={profile.summary}
            placeholder="What you do, for whom, and what you're known for"
            onChange={(e) => set("summary", e.target.value)}
          />
        </Section>

        <Section
          id="experience"
          icon="work_history"
          title="Experience"
          sub="Matches and letters quote these achievements word for word."
          onAdd={() => set("roles", [...profile.roles, newRole()])}
          addLabel="Add role"
        >
          {profile.roles.map((r) => (
            <RoleEditor
              key={r.id}
              role={r}
              allRoles={profile.roles}
              onChange={(role) =>
                set(
                  "roles",
                  profile.roles.map((x) => (x.id === r.id ? role : x)),
                )
              }
              onRemove={() =>
                set(
                  "roles",
                  profile.roles.filter((x) => x.id !== r.id),
                )
              }
            />
          ))}
          {profile.roles.length === 0 && <p className="muted">No roles added.</p>}
        </Section>

        <Section id="skills" icon="construction" title="Skills">
          <SkillsInput skills={profile.skills} onChange={(v) => set("skills", v)} />
        </Section>

        <Section
          id="education"
          icon="school"
          title="Education"
          onAdd={() =>
            set("education", [
              ...profile.education,
              { id: newId("e"), school: "", degree: "", dates: "", lines: [] },
            ])
          }
        >
          {profile.education.map((x) => (
            <ItemRow
              key={x.id}
              label={x.school || "education"}
              onRemove={() => remove("education", x.id)}
            >
              <div className="form-row form-row-3">
                <TextField
                  label="School"
                  value={x.school}
                  onChange={(v) => update("education", x.id, { school: v })}
                />
                <TextField
                  label="Degree"
                  value={x.degree}
                  onChange={(v) => update("education", x.id, { degree: v })}
                />
                <TextField
                  label="Dates"
                  value={x.dates}
                  placeholder="2014 – 2017"
                  onChange={(v) => update("education", x.id, { dates: v })}
                />
              </div>
            </ItemRow>
          ))}
          {profile.education.length === 0 && <p className="muted">No education added.</p>}
        </Section>

        <Section
          id="projects"
          icon="deployed_code"
          title="Projects & open source"
          sub="Side projects count — hiring software often misses them in sidebars."
          onAdd={() =>
            set("projects", [
              ...profile.projects,
              { id: newId("p"), name: "", link: "", description: "", lines: [] },
            ])
          }
        >
          {profile.projects.map((x) => (
            <ItemRow
              key={x.id}
              label={x.name || "project"}
              onRemove={() => remove("projects", x.id)}
            >
              <div className="form-row form-row-2">
                <TextField
                  label="Name"
                  value={x.name}
                  onChange={(v) => update("projects", x.id, { name: v })}
                />
                <TextField
                  label="Link"
                  value={x.link}
                  placeholder="github.com/…"
                  onChange={(v) => update("projects", x.id, { link: v })}
                />
              </div>
              <label className="field">
                <span className="field-label">What it is and the result</span>
                <textarea
                  rows={2}
                  value={x.description}
                  onChange={(e) => update("projects", x.id, { description: e.target.value })}
                />
              </label>
            </ItemRow>
          ))}
          {profile.projects.length === 0 && <p className="muted">No projects added.</p>}
        </Section>

        <div className="form-row form-row-2 align-start">
          <Section
            id="languages"
            icon="translate"
            title="Languages"
            onAdd={() =>
              set("languages", [
                ...profile.languages,
                { id: newId("l"), name: "", level: "", lines: [] },
              ])
            }
          >
            {profile.languages.map((x) => (
              <div className="inline-row" key={x.id}>
                <input
                  aria-label="Language"
                  placeholder="Language"
                  value={x.name}
                  onChange={(e) => update("languages", x.id, { name: e.target.value })}
                />
                <select
                  aria-label={`Level for ${x.name || "language"}`}
                  value={x.level}
                  onChange={(e) => update("languages", x.id, { level: e.target.value })}
                >
                  <option value="">Level…</option>
                  {LANGUAGE_LEVELS.map((l) => (
                    <option key={l}>{l}</option>
                  ))}
                </select>
                <button
                  type="button"
                  className="icon-button"
                  aria-label={`Remove ${x.name || "language"}`}
                  onClick={() => remove("languages", x.id)}
                >
                  <Icon name="delete" />
                </button>
              </div>
            ))}
            {profile.languages.length === 0 && <p className="muted">No languages added.</p>}
          </Section>

          <Section
            id="certs"
            icon="workspace_premium"
            title="Certifications"
            onAdd={() =>
              set("certifications", [
                ...profile.certifications,
                { id: newId("c"), name: "", issuer: "", year: "", lines: [] },
              ])
            }
          >
            {profile.certifications.map((x) => (
              <ItemRow
                key={x.id}
                label={x.name || "certification"}
                onRemove={() => remove("certifications", x.id)}
              >
                <TextField
                  label="Name"
                  value={x.name}
                  onChange={(v) => update("certifications", x.id, { name: v })}
                />
                <div className="form-row form-row-2">
                  <TextField
                    label="Issuer"
                    value={x.issuer}
                    onChange={(v) => update("certifications", x.id, { issuer: v })}
                  />
                  <TextField
                    label="Year"
                    value={x.year}
                    onChange={(v) => update("certifications", x.id, { year: v })}
                  />
                </div>
              </ItemRow>
            ))}
            {profile.certifications.length === 0 && (
              <p className="muted">No certifications added.</p>
            )}
          </Section>
        </div>
      </div>
    </section>
  );
}

type SectionProps = {
  id: string;
  icon: string;
  title: string;
  sub?: string;
  aside?: ReactNode;
  onAdd?: () => void;
  addLabel?: string;
  children: ReactNode;
};

function Section({ id, icon, title, sub, aside, onAdd, addLabel = "Add", children }: SectionProps) {
  return (
    <section id={`res-${id}`} className="resume-section" aria-labelledby={`res-${id}-title`}>
      <div className="resume-section-head">
        <div>
          <h2 id={`res-${id}-title`}>
            <Icon name={icon} />
            {title}
          </h2>
          {sub && <p>{sub}</p>}
        </div>
        {aside}
        {onAdd && (
          <button
            type="button"
            className="btn btn-secondary"
            onClick={onAdd}
            aria-label={`${addLabel} to ${title}`}
          >
            <Icon name="add" />
            {addLabel}
          </button>
        )}
      </div>
      {children}
    </section>
  );
}

function TextField(props: {
  label: string;
  value: string;
  placeholder?: string;
  onChange: (value: string) => void;
}) {
  return (
    <label className="field">
      <span className="field-label">{props.label}</span>
      <input
        value={props.value}
        placeholder={props.placeholder}
        onChange={(e) => props.onChange(e.target.value)}
      />
    </label>
  );
}

function ItemRow({
  label,
  onRemove,
  children,
}: {
  label: string;
  onRemove: () => void;
  children: ReactNode;
}) {
  return (
    <div className="card item-row">
      <div className="item-row-body">{children}</div>
      <button
        type="button"
        className="icon-button"
        aria-label={`Remove ${label}`}
        onClick={onRemove}
      >
        <Icon name="delete" />
      </button>
    </div>
  );
}
