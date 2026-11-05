import { Routes, Route, Outlet, useNavigate } from "react-router-dom";
import SidebarLink from "./components/sidebar/link";
import Autocomplete from "./pages/terminal/autocomplete";
import Onboarding from "./pages/onboarding";
import Preferences from "./pages/settings/preferences";
import Integrations from "./pages/settings/integrations";
import Keybindings from "./pages/settings/keybindings";
import Licenses from "./pages/licenses";
import { useContext, useEffect, useRef, useState } from "react";
import { Event } from "@aws/amazon-q-developer-cli-api-bindings";
import { getIconFromName } from "./lib/icons";
import { StoreContext } from "./context/zustand";
import { createStore } from "./lib/store";
import ListenerContext from "./context/input";

function App() {
  const store = useRef(createStore()).current;
  const [listening, setListening] = useState<string | null>(null);
  return (
    <StoreContext.Provider value={store}>
      <ListenerContext.Provider value={{ listening, setListening }}>
        <AppLoading />
      </ListenerContext.Provider>
    </StoreContext.Provider>
  );
}

function AppLoading() {
  const store = useContext(StoreContext);
  return store === null || store().isLoading()
    ? <div className="w-screen h-screen bg-white dark:bg-zinc-800" />
    : <Router />;
}

const navigation = [
  { name: "Getting started", link: "/" },
  { name: "CLI Completions", link: "/autocomplete" },
  { name: "Keybindings", link: "/keybindings" },
  { name: "Integrations", link: "/integrations" },
  { name: "Preferences", link: "/preferences" },
  { name: "Licenses", link: "/licenses" },
];

function Router() {
  const navigate = useNavigate();
  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let stale = false;
    Event.subscribe("dashboard.navigate", (request) => {
      if (typeof request === "object" && request !== null &&
          "path" in request && typeof request.path === "string" &&
          navigation.some(({ link }) => link === request.path)) {
        navigate(request.path);
      }
      return { unsubscribe: false };
    })?.then((result) => {
      unsubscribe = result.unsubscribe;
      if (stale) unsubscribe();
    });
    return () => { stale = true; unsubscribe?.(); };
  }, [navigate]);
  return (
    <Routes>
      <Route path="/" element={<Layout />}>
        <Route index element={<Onboarding />} />
        <Route path="autocomplete" element={<Autocomplete />} />
        <Route path="keybindings" element={<Keybindings />} />
        <Route path="integrations" element={<Integrations />} />
        <Route path="preferences" element={<Preferences />} />
        <Route path="licenses" element={<Licenses />} />
      </Route>
    </Routes>
  );
}

function Layout() {
  return (
    <div className="flex flex-row h-screen w-full overflow-hidden bg-white dark:bg-zinc-800 text-black dark:text-zinc-200">
      <nav className="w-[240px] flex-none h-full flex flex-col items-center gap-1 p-4">
        {navigation.map(({ name, link }) => (
          <SidebarLink key={link} path={link} name={name} icon={getIconFromName(name)} />
        ))}
      </nav>
      <main className="flex flex-col overflow-y-auto p-4 w-full"><Outlet /></main>
    </div>
  );
}

export default App;
