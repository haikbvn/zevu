import { UserPrefView } from "@/components/preference/list";
import settings from "@/data/integrations";

export default function Page() {
  return (
    <>
      <UserPrefView
        array={settings}
        className="w-auto self-start [&>section]:grid [&>section]:grid-cols-1 [&>section]:md:grid-cols-2 [&>section]:xl:grid-cols-3 [&>section]:2xl:grid-cols-4 [&>section>h2]:col-span-full"
      />
      <div className="flex flex-col p-4 gap-1 rounded-lg bg-zinc-50 dark:bg-zinc-900 border border-zinc-100 dark:border-zinc-700">
        <h2 className="font-bold font-ember text-lg">Terminal support</h2>
        <p>Shell and IDE integrations are installed for supported applications detected on this Mac. Terminals that require the input method show an additional setup check when focused.</p>
      </div>
    </>
  );
}
