import { useEffect, useRef, useState } from "react";
import { Icon } from "../components/Icon";
import { ResumeDrop } from "../components/ResumeDrop";
import { newRole, RoleEditor } from "../components/RoleEditor";
import { SearchForm } from "../components/SearchForm";
import { SkillsInput } from "../components/SkillsInput";
import { useToast } from "../components/Toast";
import { prefillSearch } from "../lib/profile";
import type { AppData } from "../lib/useAppData";

type SetupWizardProps = {
  data: AppData;
  onClose: () => void;
};

const STEPS = ["Resume", "Check profile", "Your search"];
const HINTS = ["About 4 minutes left", "About 3 minutes left", "Last step"];

/** Three-step first run: resume → check profile → search. Skippable at any point. */
export function SetupWizard({ data, onClose }: SetupWizardProps) {
  const toast = useToast();
  const [step, setStep] = useState(0);
  const [busy, setBusy] = useState(false);
  const heading = useRef<HTMLHeadingElement>(null);
  const prefilled = useRef(false);
  const { profile, search, updateProfile, updateSearch } = data;
  const hasResume = Boolean(data.stored?.resume);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refocus the new step's heading on step change
  useEffect(() => {
    heading.current?.focus();
  }, [step]);

  // Fill empty search fields from the profile once, when the search step opens.
  useEffect(() => {
    if (step !== 2 || prefilled.current) return;
    prefilled.current = true;
    const next = prefillSearch(search, profile);
    if (JSON.stringify(next) !== JSON.stringify(search)) updateSearch(next);
  }, [step, search, profile, updateSearch]);

  const finish = async (status: "done" | "skipped") => {
    setBusy(true);
    try {
      await data.finishSetup(status);
      if (status === "done") toast("Setup done. Your profile and search are saved.");
      onClose();
    } catch (err) {
      toast(`Couldn't save setup: ${err}`, "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="wizard">
      <div className="wizard-inner">
        <div className="wizard-top">
          <div className="brand-name">
            <Icon name="travel_explore" />
            JobLens <span className="muted">· Quick setup, about 5 minutes</span>
          </div>
          <button
            type="button"
            className="btn btn-ghost"
            disabled={busy}
            onClick={() => finish("skipped")}
          >
            Skip for now
            <Icon name="close" />
          </button>
        </div>

        <ol className="wizard-steps">
          {STEPS.map((label, i) => (
            <li
              key={label}
              data-done={i <= step || undefined}
              aria-current={i === step ? "step" : undefined}
            >
              {i + 1}. {label}
            </li>
          ))}
        </ol>

        {step === 0 && (
          <div className="wizard-step">
            <header>
              <h1 ref={heading} tabIndex={-1}>
                Start with your resume
              </h1>
              <p>We read it into a profile you check once. The file never leaves this computer.</p>
            </header>
            <ResumeDrop
              resume={data.stored?.resume ?? null}
              profile={profile}
              onImport={data.importResume}
            />
            {!hasResume && (
              <button type="button" className="btn btn-link" onClick={() => setStep(1)}>
                No resume handy? Fill in your profile by hand
              </button>
            )}
          </div>
        )}

        {step === 1 && (
          <div className="wizard-step">
            <header>
              <h1 ref={heading} tabIndex={-1}>
                Check what we found
              </h1>
              <p>
                Fix anything we got wrong. Every match and letter is built from this, and nothing
                else.
              </p>
            </header>
            {profile.roles.map((r) => (
              <RoleEditor
                key={r.id}
                role={r}
                allRoles={profile.roles}
                onChange={(role) =>
                  updateProfile({
                    ...profile,
                    roles: profile.roles.map((x) => (x.id === r.id ? role : x)),
                  })
                }
                onRemove={() =>
                  updateProfile({ ...profile, roles: profile.roles.filter((x) => x.id !== r.id) })
                }
              />
            ))}
            {profile.roles.length === 0 && (
              <p className="muted">
                {hasResume
                  ? "We couldn't find any roles in your resume. Add them here, or later on the Resume page."
                  : "Add your recent roles. You can fill in the rest later on the Resume page."}
              </p>
            )}
            <button
              type="button"
              className="btn btn-secondary align-self-start"
              onClick={() => updateProfile({ ...profile, roles: [...profile.roles, newRole()] })}
            >
              <Icon name="add" />
              Add role
            </button>
            <SkillsInput
              skills={profile.skills}
              onChange={(skills) => updateProfile({ ...profile, skills })}
            />
          </div>
        )}

        {step === 2 && (
          <div className="wizard-step">
            <header>
              <h1 ref={heading} tabIndex={-1}>
                What are you looking for?
              </h1>
              <p>You can change all of this later in Settings.</p>
            </header>
            <SearchForm value={search} onChange={updateSearch} />
          </div>
        )}

        <div className="wizard-footer">
          {step > 0 && (
            <button type="button" className="btn btn-secondary" onClick={() => setStep(step - 1)}>
              <Icon name="arrow_back" />
              Back
            </button>
          )}
          <span className="wizard-hint">{HINTS[step]}</span>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || (step === 0 && !hasResume)}
            onClick={() => (step < 2 ? setStep(step + 1) : finish("done"))}
          >
            {step < 2 ? "Continue" : "Start searching"}
            <Icon name="arrow_forward" />
          </button>
        </div>
      </div>
    </div>
  );
}
