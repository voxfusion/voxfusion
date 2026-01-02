import { render } from "solid-js/web";
import { Route, Router } from "@solidjs/router";
import "./styles.css";
import Home from "./pages/Home";
import About from "./pages/About";
import Settings from "./pages/Settings";
import App from "./App";

const root = document.getElementById("root");

if (!root) {
	throw new Error("Root element not found");
}

render(
	() => (
		<Router root={App}>
			<Route path="/" component={Home} />
			<Route path="/about" component={About} />
			<Route path="/settings" component={Settings} />
		</Router>
	),
	root
);
