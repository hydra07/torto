import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./style.css";

// This harness owns stateful WASM/GPU resources. React's development-only
// StrictMode remount starts two async canvas initializations before the first
// can be cancelled, which can re-enter wasm-bindgen's exclusive object guard.
createRoot(document.getElementById("root")!).render(<App />);
