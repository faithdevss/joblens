import { useId } from "react";
import type { SearchSettings } from "../bindings/SearchSettings";
import { COUNTRIES, CURRENCIES, TIMEZONES } from "../lib/places";
import { SENIORITY } from "../lib/profile";
import { ChipInput } from "./ChipInput";

type SearchFormProps = {
  value: SearchSettings;
  onChange: (value: SearchSettings) => void;
};

export function SearchForm({ value, onChange }: SearchFormProps) {
  const id = useId();
  const set = <K extends keyof SearchSettings>(key: K, v: SearchSettings[K]) =>
    onChange({ ...value, [key]: v });

  const timezones = TIMEZONES.some((t) => t.value === value.timezone)
    ? TIMEZONES
    : [...TIMEZONES, { value: value.timezone, label: value.timezone }];

  return (
    <div className="form-stack">
      <ChipInput
        label="Job titles"
        values={value.titles}
        onChange={(v) => set("titles", v)}
        placeholder="Type a title, press Enter…"
        hint="Press Enter to add. Matching titles rank higher."
      />
      <div className="form-row">
        <ChipInput
          label="Must-have keywords"
          values={value.mustHave}
          onChange={(v) => set("mustHave", v)}
          placeholder="Type a keyword, press Enter or comma…"
          hint="Jobs without these are hidden."
        />
        <ChipInput
          label="Keywords to avoid"
          values={value.avoid}
          onChange={(v) => set("avoid", v)}
          placeholder="Type a keyword, press Enter or comma…"
          hint="Jobs mentioning these are hidden."
        />
      </div>
      <div className="form-row form-row-3">
        <div className="field">
          <label className="field-label" htmlFor={`${id}-country`}>
            Your country
          </label>
          <select
            id={`${id}-country`}
            value={value.country}
            onChange={(e) => set("country", e.target.value)}
          >
            <option value="">Choose…</option>
            {COUNTRIES.map((c) => (
              <option key={c.code} value={c.code}>
                {c.name}
              </option>
            ))}
          </select>
          <span className="field-hint">Used for the “Can I apply?” badge.</span>
        </div>
        <label className="field">
          <span className="field-label">Your timezone</span>
          <select value={value.timezone} onChange={(e) => set("timezone", e.target.value)}>
            {value.timezone === "" && <option value="">Choose…</option>}
            {timezones.map((t) => (
              <option key={t.value} value={t.value}>
                {t.label}
              </option>
            ))}
          </select>
        </label>
        <div className="field">
          <label className="field-label" htmlFor={`${id}-salary`}>
            Minimum salary
          </label>
          <div className="input-group">
            <input
              id={`${id}-salary`}
              type="number"
              min={0}
              step={5000}
              placeholder="Any"
              value={value.minSalary ?? ""}
              onChange={(e) => {
                const n = Math.max(0, Math.round(Number(e.target.value)));
                set("minSalary", e.target.value === "" || Number.isNaN(n) ? null : n);
              }}
            />
            <select
              aria-label="Currency"
              value={value.currency}
              onChange={(e) => set("currency", e.target.value)}
            >
              {CURRENCIES.map((c) => (
                <option key={c}>{c}</option>
              ))}
            </select>
          </div>
          <span className="field-hint">Per year. “Not stated” jobs still show.</span>
        </div>
      </div>
      <fieldset className="field">
        <legend className="field-label">Seniority</legend>
        <div className="toggle-group">
          {SENIORITY.map((s) => {
            const on = value.seniority.includes(s.value);
            return (
              <button
                key={s.value}
                type="button"
                className="toggle"
                aria-pressed={on}
                onClick={() =>
                  set(
                    "seniority",
                    on
                      ? value.seniority.filter((x) => x !== s.value)
                      : SENIORITY.map((o) => o.value).filter(
                          (v) => v === s.value || value.seniority.includes(v),
                        ),
                  )
                }
              >
                {s.label}
              </button>
            );
          })}
        </div>
      </fieldset>
    </div>
  );
}
