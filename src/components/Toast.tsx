import { createContext, type ReactNode, useCallback, useContext, useRef, useState } from "react";

type Tone = "info" | "error";
type ToastState = { message: string; tone: Tone } | null;
type ShowToast = (message: string, tone?: Tone) => void;

const ToastContext = createContext<ShowToast>(() => {});

const DURATION_MS = 3200;

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toast, setToast] = useState<ToastState>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  const show = useCallback<ShowToast>((message, tone = "info") => {
    clearTimeout(timer.current);
    setToast({ message, tone });
    timer.current = setTimeout(() => setToast(null), DURATION_MS);
  }, []);

  return (
    <ToastContext.Provider value={show}>
      {children}
      {toast && (
        <div className="toast" role="status" data-tone={toast.tone}>
          {toast.message}
        </div>
      )}
    </ToastContext.Provider>
  );
}

export function useToast(): ShowToast {
  return useContext(ToastContext);
}
