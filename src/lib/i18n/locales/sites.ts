// Registra el español del área diferida «sites». Lo importa su vista (y cualquier
// módulo que use claves «sites.*» fuera de ella): así viaja en su chunk.
import { registerArea } from "../translate";
import es from "./es/sites";

registerArea("sites", es);
