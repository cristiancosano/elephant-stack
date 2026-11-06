// Registra el español del área diferida «agents». Lo importa su vista (y cualquier
// módulo que use claves «agents.*» fuera de ella): así viaja en su chunk.
import { registerArea } from "../translate";
import es from "./es/agents";

registerArea("agents", es);
