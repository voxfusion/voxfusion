import { Route, Router } from "@solidjs/router";
import { ErrorBoundary } from "solid-js";
import { render } from "solid-js/web";
import "./styles.css";
import App from "./App";
import { I18nCtx, createAppI18n, getStoredLocale } from "./i18n";
import { initPostHog } from "./lib/posthog";
import About from "./pages/About";
import Dictionary from "./pages/Dictionary";
import Home from "./pages/Home";

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
			<ErrorBoundary
				fallback={(err) => (
					<div class="h-full flex flex-col items-center justify-center font-mono p-8">
						<p class="text-ac mb-2">[FATAL ERROR]</p>
						<p class="text-sm text-txt-secondary mb-4">
							{err?.message ?? "An unexpected error occurred"}
						</p>
						<button
							type="button"
							onClick={() => window.location.reload()}
							class="text-ac hover:underline text-sm"
						>
							[RELOAD]
						</button>
					</div>
				)}
			>
				<Router root={App}>
					<Route path="/" component={Home} />
					<Route path="/about" component={About} />
					<Route path="/dictionary" component={Dictionary} />
				</Router>
			</ErrorBoundary>
		</I18nCtx.Provider>
	),
	root
);
