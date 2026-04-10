# Plan de Test v2

## Context

Plan sencillo de test para validar el flujo completo de plan mode → aprobación → ejecución en cluihud.

## Tasks

### Task 1: Crear archivo de configuración dummy
- Crear `test_config.yaml` con estructura `version: 1`, `theme: light`, `panels: [terminal, tasks]`
- Verificar que el YAML es válido con `yq`

### Task 2: Validar estructura del JSON
- Leer `test_config.json` y verificar que contiene las keys esperadas (`version`, `theme`, `panels`)
- Confirmar que `panels` es un array con exactamente 2 elementos

### Task 3: Generar backup y comparar checksums
- Copiar `test_config.json` a `test_config.backup.json`
- Calcular checksum MD5 de ambos archivos
- Confirmar que los checksums son idénticos

## Verification
- `test_config.yaml` se crea con YAML válido
- Validación de estructura pasa correctamente
- Backup tiene checksum idéntico al original
