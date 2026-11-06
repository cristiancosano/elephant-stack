// Registra el español del área diferida «wizard». Lo importa su vista (y cualquier
// módulo que use claves «wizard.*» fuera de ella): así viaja en su chunk.
import { registerArea } from "../translate";
import es from "./es/wizard";

registerArea("wizard", es);
