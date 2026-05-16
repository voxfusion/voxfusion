import { Route, Router } from "@solidjs/router";
import { render } from "solid-js/web";
import "./styles.css";
import App from "./App";
import { I18nCtx, createAppI18n, getStoredLocale } from "./i18n";
import { initPostHog } from "./lib/posthog";
import About from "./pages/About";
import Dictionary from "./pages/Dictionary";
import DictionaryDefault from "./pages/DictionaryDefault";
import DictionaryPerApp from "./pages/DictionaryPerApp";
import Home from "./pages/Home";
import Style from "./pages/Style";

initPostHog();

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
				</Route>
				<Route path="/style" component={Style} />
			</Router>
		</I18nCtx.Provider>
	),
	root
);
