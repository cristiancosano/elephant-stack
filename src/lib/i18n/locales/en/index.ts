// Todo el inglés en un chunk: solo se descarga si la interfaz está en inglés.
import type { Messages } from "../index";
import type { Dictionary } from "../../types";
import agents from "./agents";
import common from "./common";
import logs from "./logs";
import node from "./node";
import panel from "./panel";
import php from "./php";
import services from "./services";
import settings from "./settings";
import shell from "./shell";
import sites from "./sites";
import ui from "./ui";
import wizard from "./wizard";

export default {
  common,
  shell,
  ui,
  panel,
  services,
  php,
  node,
  sites,
  logs,
  wizard,
  settings,
  agents,
} satisfies Dictionary<Messages>;
