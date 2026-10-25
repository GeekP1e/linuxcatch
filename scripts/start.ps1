$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    throw 'Установи Docker Desktop с WSL 2. Автоматический установщик будет добавлен отдельно.'
}

docker info | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Запусти Docker Desktop в режиме Linux-контейнеров.' }
docker compose version | Out-Null
if ($LASTEXITCODE -ne 0) { throw 'Docker Compose недоступен.' }
docker compose up --build -d --wait
if ($LASTEXITCODE -ne 0) { throw 'Запуск не удался. Проверь вывод Docker Compose.' }
Start-Process 'http://localhost:8080'
