import { useState } from "react";
import checks from "@/data/install";
import { useAccessibilityCheck, useDotfilesCheck, useInputMethodCheck } from "@/hooks/store";
import { useLocalStateZodDefault } from "@/hooks/store/useState";
import { z } from "zod";
import InstallModal from "./install";

export default function OnboardingModal() {
  const [shell] = useDotfilesCheck();
  const [accessibility] = useAccessibilityCheck();
  const [inputMethod] = useInputMethodCheck();
  const [inputMethodRequired] = useLocalStateZodDefault("desktop.inputMethodRequired", z.boolean(), false);
  const [revision, setRevision] = useState(0);
  const missing = shell !== true ? "dotfiles" : accessibility !== true ? "accessibility" : inputMethodRequired && inputMethod !== true ? "inputMethod" : undefined;
  const check = checks.find(({ id }) => id === missing);
  return check ? <InstallModal key={`${check.id}-${revision}`} check={check} next={() => setRevision((value) => value + 1)} /> : null;
}
