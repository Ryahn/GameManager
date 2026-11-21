import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Lock } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/primitives";
import { api } from "@/lib/ipc";
import { friendlyError } from "@/lib/errors";
import { Logo } from "./Sidebar";

/** Shown instead of the app while the PIN lock is on. */
export function LockScreen() {
  const qc = useQueryClient();
  const [pin, setPin] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const unlock = async () => {
    setBusy(true);
    setError(null);
    try {
      if (await api.pinUnlock(pin)) {
        qc.invalidateQueries({ queryKey: ["system-info"] });
      } else {
        setError("That PIN isn't right.");
        setPin("");
      }
    } catch (e) {
      setError(friendlyError(e).detail);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="grid h-full place-items-center bg-bg">
      <form
        className="w-[min(90vw,340px)] text-center animate-fade-up"
        onSubmit={(e) => {
          e.preventDefault();
          void unlock();
        }}
      >
        <div className="mb-6 flex justify-center">
          <Logo />
        </div>
        <div className="mx-auto mb-4 grid size-12 place-items-center rounded-2xl bg-accent/12 text-accent">
          <Lock className="size-6" />
        </div>
        <h1 className="text-lg font-semibold">Locked</h1>
        <p className="mt-1 text-sm text-muted">Enter your PIN to continue.</p>
        <Input
          autoFocus
          type="password"
          inputMode="numeric"
          autoComplete="off"
          value={pin}
          onChange={(e) => setPin(e.target.value.replace(/\D/g, "").slice(0, 8))}
          aria-label="PIN"
          className="mt-5 h-11 text-center text-lg tracking-[0.4em]"
        />
        {error && (
          <p role="alert" className="mt-2 text-sm text-danger">
            {error}
          </p>
        )}
        <Button type="submit" variant="primary" size="lg" className="mt-4 w-full" disabled={pin.length < 4} loading={busy}>
          Unlock
        </Button>
      </form>
    </div>
  );
}
