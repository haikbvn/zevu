import autocompleteDemo from "@assets/images/autocomplete_demo.gif";
import { useEffect, useState } from "react";
import { Fig, Internal, State } from "@aws/amazon-q-developer-cli-api-bindings";
import { Terminal } from "@/components/ui/terminal";
import { Button } from "@/components/ui/button";
import Modal from "@/components/modal";
import InstallModal from "@/components/installs/modal";
import StatusCheck from "@/components/installs/statusCheck";
import checks from "@/data/install";
import { useAccessibilityCheck, useDotfilesCheck, useInputMethodCheck } from "@/hooks/store";
import { useLocalStateZodDefault, useRefreshLocalState } from "@/hooks/store/useState";
import { z } from "zod";

export default function Page() {
  const [shell, refreshShell] = useDotfilesCheck();
  const [accessibility, refreshAccessibility] = useAccessibilityCheck();
  const [inputMethod, refreshInputMethod] = useInputMethodCheck();
  const [inputMethodRequired] = useLocalStateZodDefault("desktop.inputMethodRequired", z.boolean(), false);
  const [complete] = useLocalStateZodDefault("desktop.completedOnboarding", z.boolean(), false);
  const refreshState = useRefreshLocalState();
  const [repair, setRepair] = useState(false);
  const [error, setError] = useState("");
  const [checksCurrent, setChecksCurrent] = useState(false);
  const localChecksPass = checksCurrent && shell === true && accessibility === true && (!inputMethodRequired || inputMethod === true);
  const ready = localChecksPass && complete;

  useEffect(() => {
    let active = true;
    const refresh = async () => {
      try {
        await Promise.all([refreshShell(), refreshAccessibility(), refreshInputMethod(), refreshState()]);
        if (active) {
          setChecksCurrent(true);
          setError("");
        }
      } catch (err) {
        if (active) {
          setChecksCurrent(false);
          setError(String(err));
        }
      }
    };
    void refresh();
    const timer = setInterval(() => void refresh(), 2000);
    return () => { active = false; clearInterval(timer); };
  }, [refreshShell, refreshAccessibility, refreshInputMethod, refreshState]);

  useEffect(() => {
    if (!localChecksPass) {
      if (complete && checksCurrent) {
        State.set("desktop.completedOnboarding", false)
          .then(() => refreshState())
          .catch((err) => setError(String(err)));
      }
      return;
    }
    setRepair(false);
    if (!complete) {
      Internal.sendOnboardingRequest({ action: Fig.OnboardingAction.FINISH_ONBOARDING })
        .then(() => refreshState())
        .catch((err) => setError(String(err)));
    }
  }, [localChecksPass, checksCurrent, complete, refreshState]);

  return (
    <div className="flex flex-col items-start gap-4">
      <div className="flex flex-col"><h1 className="text-2xl font-bold dark:text-zinc-400">Getting started</h1></div>
      <div className="flex flex-col gap-2 w-full border bg-zinc-50 border-zinc-200 dark:bg-zinc-900 dark:border-zinc-600 rounded-lg p-4 mb-4 overflow-x-auto">
        <h2 className="font-bold font-ember tracking-tight text-lg dark:text-zinc-300">{ready ? "Zevu is ready" : "Zevu setup status"}</h2>
        <p className="text-sm text-zinc-600 dark:text-zinc-400">{ready ? "Open a new shell session to use autocomplete. Customize appearance and keybindings in the sidebar." : "Complete the local checks below to enable autocomplete. No account is required."}</p>
        {error && <p role="alert" className="text-sm text-red-500">{error}</p>}
        {!ready && <Button onClick={() => setRepair(true)}>Repair setup</Button>}
      </div>
      {checks.filter((check) => check.id !== "inputMethod" || inputMethodRequired).map((check) => <StatusCheck key={check.id} check={check} />)}
      <Terminal tabNames={["Autocomplete"]}><Terminal.Tab><img src={autocompleteDemo} alt="Autocomplete demo" /></Terminal.Tab></Terminal>
      {repair && !ready && <Modal><InstallModal /></Modal>}
    </div>
  );
}
