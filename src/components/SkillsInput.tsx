import type { Skill } from "../bindings/Skill";
import { newId } from "../lib/profile";
import { ChipInput } from "./ChipInput";

type SkillsInputProps = {
  skills: Skill[];
  onChange: (skills: Skill[]) => void;
};

/** Chip editor over Skill objects; keeps ids and source lines of skills that stay. */
export function SkillsInput({ skills, onChange }: SkillsInputProps) {
  return (
    <ChipInput
      label="Skills"
      values={skills.map((s) => s.name)}
      placeholder="Add a skill…"
      hint="Only list what you'd be comfortable discussing in an interview."
      onChange={(names) =>
        onChange(
          names.map(
            (name) => skills.find((s) => s.name === name) ?? { id: newId("s"), name, lines: [] },
          ),
        )
      }
    />
  );
}
