import React from "react";
import ReactDOM from "react-dom/client";
import PillApp from "./PillApp";
import "./pill.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <PillApp />
  </React.StrictMode>,
);
