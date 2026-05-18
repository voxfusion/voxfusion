import { Route, Router } from "@solidjs/router";
import { render } from "solid-js/web";
import "./styles.css";
import App from "./App";
import { I18nCtx, createAppI18n, getStoredLocale } from "./i18n";
import { initPostHog } from "./lib/posthog";
import { disableContextMenu } from "./lib/contextMenu";
import About from "./pages/About";
import Dictionary from "./pages/Dictionary";
import DictionaryDefault from "./pages/DictionaryDefault";
import DictionaryPerApp from "./pages/DictionaryPerApp";
import DictionarySites from "./pages/DictionarySites";
import Home from "./pages/Home";
import Style from "./pages/Style";
import StyleDefault from "./pages/StyleDefault";
import StylePerApp from "./pages/StylePerApp";
import StylePerSite from "./pages/StylePerSite";

initPostHog();
disableContextMenu();

const root = document.getElementById("root");

if (!root) {
	throw new Error("Root element not found");
}

const initialLocale = getStoredLocale();
const i18n = createAppI18n(initialLocale);

render(
	() => (
		<I18nCtx.Provider value={i18n}>
			<Router root={App}>
				<Route path="/" component={Home} />
				<Route path="/about" component={About} />
				<Route path="/dictionary" component={Dictionary}>
					<Route path="/" component={DictionaryDefault} />
					<Route path="/per-app" component={DictionaryPerApp} />
					<Route path="/sites" component={DictionarySites} />
				</Route>
				<Route path="/style" component={Style}>
					<Route path="/" component={StyleDefault} />
					<Route path="/per-app" component={StylePerApp} />
					<Route path="/sites" component={StylePerSite} />
				</Route>
			</Router>
		</I18nCtx.Provider>
	),
	root
);
