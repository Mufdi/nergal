# Plan de Test v2

## Context

Plan sencillo de test para validar el flujo completo de plan mode → aprobación → ejecución en cluihud.

> Nota (anotación global): feedback de test recibido y registrado.

## Tasks

> Nota: feedback de test recibido sobre la sección de tasks.

### Task 1: Crear archivo de configuración dummy
- eeee
- Verificar que el archivo es válido

### Task 2: Validar estructura del JSON
- Leer `test_config.json` y verificar que contiene las keys esperadas (`version`, `theme`, `panels`).
  Ejemplo de validación con `jq`:
  ```bash
  jq 'has("version") and has("theme") and has("panels")' test_config.json
  # → true
  ```
- Confirmar que `panels` es un array con exactamente 2 elementos

### Task 3: Generar backup
- Copiar `test_config.json` a `test_config.backup.json`
- Confirmar que el backup existe

## Verification
- Archivo de configuración se crea válido
- Validación de estructura pasa correctamente
- Backup se genera correctamente
