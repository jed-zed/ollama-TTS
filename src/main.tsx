import { createRoot } from "react-dom/client";
import App from "./App.tsx";
import { initializeBackendBaseUrl } from "@/lib/backendUrl";
import "./index.css";

const bootstrap = async () => {
  await initializeBackendBaseUrl();
  createRoot(document.getElementById("root")!).render(<App />);
};

void bootstrap();
