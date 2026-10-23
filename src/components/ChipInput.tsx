import { type KeyboardEvent, useState } from "react";

type ChipInputProps = {
  label: string;
  values: string[];
  onChange: (values: string[]) => void;
  placeholder?: string;
  hint?: string;
};

/** Free-text tags. Enter or comma adds, Backspace on an empty box removes the last one. */
export function ChipInput({ label, values, onChange, placeholder, hint }: ChipInputProps) {
  const [draft, setDraft] = useState("");

  const add = () => {
    const next = [...values];
    for (const raw of draft.split(",")) {
      const v = raw.trim();
      if (v && !next.some((x) => x.toLowerCase() === v.toLowerCase())) next.push(v);
    }
    if (next.length !== values.length) onChange(next);
    setDraft("");
  };

  const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Enter" || e.key === ",") {
      e.preventDefault();
      add();
    } else if (e.key === "Backspace" && draft === "" && values.length > 0) {
      onChange(values.slice(0, -1));
    }
  };

  return (
    <fieldset className="field">
      <legend className="field-label">{label}</legend>
      <div className="chips">
        {values.map((v) => (
          <span className="chip" key={v}>
            {v}
            <button
              type="button"
              aria-label={`Remove ${v}`}
              onClick={() => onChange(values.filter((x) => x !== v))}
            >
              ×
            </button>
          </span>
        ))}
        <input
          aria-label={label}
          value={draft}
          placeholder={placeholder}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={onKeyDown}
          onBlur={add}
        />
      </div>
      {hint && <span className="field-hint">{hint}</span>}
    </fieldset>
  );
}
