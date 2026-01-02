import { render } from "solid-js/web";
import "./styles.css";
import VoiceControl from "./pages/VoiceControl";

const root = document.getElementById("root");

if (!root) {
	throw new Error("Root element not found");
}

render(() => <VoiceControl />, root);

