// Registra el español del área diferida «logs». Lo importa su vista (y cualquier
// módulo que use claves «logs.*» fuera de ella): así viaja en su chunk.
import { registerArea } from "../translate";
import es from "./es/logs";

registerArea("logs", es);
