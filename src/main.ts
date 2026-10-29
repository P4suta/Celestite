import "@fontsource/ibm-plex-mono/latin-400.css";
import "@fontsource/ibm-plex-mono/latin-700.css";
import "@fontsource/noto-sans-jp/japanese-400.css";
import "@fontsource/noto-sans-jp/japanese-700.css";
import "@fontsource/inter/latin-400.css";
import "@fontsource/inter/latin-700.css";
import "./style.css";

import { mount } from "svelte";
import App from "./App.svelte";

if (import.meta.env.MODE === "test") {
  await import("../tests/browser/native-mock");
}

const target = document.getElementById("app");
if (!target) throw new Error("The application root is missing.");
mount(App, { target });
