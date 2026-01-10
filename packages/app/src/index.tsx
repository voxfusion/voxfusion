import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";
import "./styles.css";
import Home from "./pages/Home";
import About from "./pages/About";
import Dictionary from "./pages/Dictionary";
import App from "./App";
import { I18nCtx, createAppI18n, getStoredLocale } from "./i18n";

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
				<Route path="/dictionary" component={Dictionary} />
			</Router>
		</I18nCtx.Provider>
	),
	root
);
