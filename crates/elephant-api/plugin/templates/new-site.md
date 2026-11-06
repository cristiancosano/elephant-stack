Crea un sitio Laravel nuevo llamado {{name}} con la orden `elephant` de Elephant Stack.

1. `elephant php list`: elige la versión de PHP más reciente instalada, salvo que se pida otra.
2. `elephant project create {{name}} --php <versión> --detach`, y después `elephant job show <ID> --wait 60` hasta que termine.
3. `elephant project start-services --dir <ruta que devuelve>`: arranca lo que pide su `.env`.
4. Comprueba con `elephant logs errors --project {{name}} --since 10m` que no hay errores y da la URL (`http://{{name}}.test` o la que devuelva).

El nombre solo admite minúsculas, números y guiones. La creación muestra un diálogo de Elephant a la persona; si lo deniega, no lo intentes por otra vía.
