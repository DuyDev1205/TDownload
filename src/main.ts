import { mount } from "svelte";
import { isTauri } from "@tauri-apps/api/core";
import "./app.css";
import "./lib/theme";
import App from "./App.svelte";

// Chạy `npm run dev` trên trình duyệt thì dùng backend giả để chỉnh giao diện; bản build không chứa phần này.
const ready = import.meta.env.DEV && !isTauri() ? import("./lib/mock").then((m) => m.install()) : Promise.resolve();

ready.then(() => mount(App, { target: document.getElementById("app")! }));
