# Bateria de medidas da entrega 0.6 (pedido do dono, 2026-10-30).
# Roda de novo todas as medições já feitas na 0.6 (operações, junção,
# abertura, memória, disco, índice, verificação), compara com os limites
# aprovados e com a rodada anterior, e grava o resultado em
# prototipo/resultados/bateria/, que fica no repositório (pedido do dono):
#   <data>_<hora>.txt  o registro completo de cada rodada;
#   historico.csv      uma linha por medida, de todas as rodadas;
#   ultima.json        a referência da próxima comparação.
# Faça o commit desses arquivos junto com a parte em que a rodada foi feita.
#
# Uso, a partir da pasta do projeto:
#   powershell -ExecutionPolicy Bypass -File poc/crdt-armazenamento/medir-tudo.ps1
#
# Saída: 0 = tudo dentro dos limites e nada piorou; 1 = algum limite
# passou ou alguma medida falhou; 2 = dentro dos limites, mas algo piorou
# mais de 25% em relação à rodada anterior.

param(
    # Pasta no disco rígido (A:) e no SSD (C:) para as medidas de disco.
    [string]$PastaHD = (Join-Path $PSScriptRoot 'prototipo\resultados\tmp-hd'),
    [string]$PastaSSD = (Join-Path $env:TEMP 'bivar-medidas'),
    # O teste do arquivo de 2 GB é pulado num disco com menos espaço livre.
    [double]$EspacoMinimoGB = 6
)

$ErrorActionPreference = 'Continue'
$proto = Join-Path $PSScriptRoot 'prototipo'
$manifest = Join-Path $proto 'Cargo.toml'
$saida = Join-Path $proto 'resultados\bateria'
New-Item -ItemType Directory -Force $saida | Out-Null
$inicio = Get-Date
$carimbo = $inicio.ToString('yyyy-MM-dd_HHmm')
$log = Join-Path $saida "$carimbo.txt"
$inv = [System.Globalization.CultureInfo]::InvariantCulture

# Piorou = mais de 25% acima da rodada anterior E mais que a folga da
# medida (medidas pequenas e de disco variam por acaso).
$PIORA = 1.25

function M($chave, $nome, $padrao, $unidade, $limite, $folga) {
    [pscustomobject]@{ Chave = $chave; Nome = $nome; Padrao = $padrao; Unidade = $unidade; Limite = $limite; Folga = $folga }
}

function Disco($pasta, $rotulo) {
    $raiz = [System.IO.Path]::GetPathRoot((Resolve-Path -LiteralPath (New-Item -ItemType Directory -Force $pasta)).Path)
    $livre = (Get-PSDrive ($raiz.Substring(0, 1))).Free / 1GB
    $medidas = @(
        @{ Teste = 'sync_cost'; Metricas = @(
                (M "$rotulo.gravar_no_disco_ms" "Forçar gravação no disco, mediana ($rotulo)" 'sync_ms median=([\d.]+)' 'ms' $null 10)) },
        @{ Teste = 'autosave_pause'; Metricas = @(
                (M "$rotulo.pausa_autosave_ms" "Pausa do salvamento automático, pior caso ($rotulo)" 'autosave_on_interface_thread_ms max=([\d.]+)' 'ms' 16 2)) },
        @{ Teste = 'growth_and_compaction'; Metricas = @(
                (M "$rotulo.arquivo_compactado_mb" "Arquivo depois da compactação, 200 mil operações ($rotulo)" 'file_after_mb=([\d.]+)' 'MB' $null 0.5),
                (M "$rotulo.compactar_s" "Tempo de compactar ($rotulo)" 'compact_s=([\d.]+)' 's' $null 0.5)) }
    )
    if ($livre -ge $EspacoMinimoGB) {
        $medidas += @{ Teste = 'blob_2gb'; Metricas = @(
                (M "$rotulo.arquivo_2gb_gravar_s" "Arquivo grande de 2 GB: guardar ($rotulo)" 'write_and_save_s=([\d.]+)' 's' $null 5),
                (M "$rotulo.arquivo_2gb_abrir_s" "Arquivo grande de 2 GB: abrir o documento ($rotulo)" 'open_s=([\d.]+)' 's' $null 0.5),
                (M "$rotulo.arquivo_2gb_ler_s" "Arquivo grande de 2 GB: ler e conferir ($rotulo)" 'read_and_check_s=([\d.]+)' 's' $null 2)) }
    } else {
        $script:pulados += "Arquivo grande de 2 GB ($rotulo): só $([math]::Round($livre, 1)) GB livres, mínimo $EspacoMinimoGB GB"
    }
    foreach ($m in $medidas) {
        $m.Pacote = 'bivar-poc-arquivo'; $m.Arquivo = 'medidas'; $m.Pasta = $pasta
    }
    $medidas
}

$script:pulados = @()
$bateria = @(
    @{ Pacote = 'bivar-poc-loja-loro'; Arquivo = 'measure'; Teste = 'ops_100k_entries'; Metricas = @(
            (M 'operacoes_100k_s' '100 mil operações' 'ops_100k_median_seconds=([\d.]+)' 's' 5 0.3)) },
    @{ Pacote = 'bivar-poc-loja-loro'; Arquivo = 'measure'; Teste = 'merge_entries'; Metricas = @(
            (M 'juncao_10k_10k_s' 'Junção de 10 mil + 10 mil operações' 'merge_10k_plus_10k_median_seconds=([\d.]+)' 's' 1 0.1)) },
    @{ Pacote = 'bivar-poc-loja-loro'; Arquivo = 'measure'; Teste = 'build_1m_entries'; Metricas = @(
            (M 'gerar_1m_s' 'Gerar o documento de 1 milhão de operações' 'build_1m_seconds=([\d.]+)' 's' $null 5),
            (M 'tamanho_1m_mb' 'Tamanho desse documento' 'file_bytes=(\d+)' 'bytes' $null 1048576)) },
    @{ Pacote = 'bivar-poc-loja-loro'; Arquivo = 'measure'; Teste = 'open_1m_entries'; Metricas = @(
            (M 'abrir_ler_1m_s' 'Abrir e ler 1 milhão de operações' 'total=([\d.]+)' 's' 3 0.3),
            (M 'memoria_1m_mb' 'Memória para abrir e ler 1 milhão de operações' 'extra_memory_mb=(\d+)' 'MB' 1024 50)) },
    @{ Pacote = 'bivar-poc-loja-loro'; Arquivo = 'measure'; Teste = 'hidden_index_deep'; Metricas = @(
            (M 'indice_leitura_us' 'Ler um nó a 10 mil níveis, com o índice' 'per_read_with_index_us=([\d.]+)' 'µs' $null 5)) },
    @{ Pacote = 'bivar-poc-verificador'; Arquivo = 'entrada'; Teste = 'verification_cost_on_a_large_document'; Metricas = @(
            (M 'abrir_confiavel_s' 'Abrir arquivo deste computador (1 milhão de operações)' 'Trusted in ([\d.]+) s' 's' $null 0.2),
            (M 'abrir_de_fora_s' 'Abrir arquivo de fora, com verificação' 'Verified in ([\d.]+) s' 's' $null 0.5)) }
)
$bateria += Disco $PastaSSD 'SSD C:'
$bateria += Disco $PastaHD 'HD A:'

Write-Host "Compilando as medidas (modo otimizado)..."
& cargo test --release --manifest-path $manifest --workspace --no-run 2>&1 | Out-File -Encoding utf8 $log
if ($LASTEXITCODE -ne 0) { Write-Host "A compilação falhou; veja $log"; exit 1 }

$valores = [ordered]@{}
$falhas = @()
foreach ($b in $bateria) {
    Write-Host "Medindo: $($b.Teste) $(if ($b.Pasta) { "em $($b.Pasta)" })"
    if ($b.Pasta) { $env:BIVAR_TEST_DIR = $b.Pasta } else { Remove-Item Env:BIVAR_TEST_DIR -ErrorAction SilentlyContinue }
    $texto = & cargo test --release --manifest-path $manifest -p $b.Pacote --test $b.Arquivo $b.Teste -- --ignored --exact --nocapture --test-threads=1 2>&1 | ForEach-Object { "$_" }
    $ok = $LASTEXITCODE -eq 0
    Remove-Item Env:BIVAR_TEST_DIR -ErrorAction SilentlyContinue
    # O registro vai para o repositório público: sem a pasta do usuário.
    "### $($b.Teste) $($b.Pasta)", $texto | ForEach-Object { $_ } |
        ForEach-Object { $_.Replace($env:TEMP, '%TEMP%') } |
        Out-File -Append -Encoding utf8 $log
    # O Rust pode escrever "test <nome> ... RESULT ..." numa linha só.
    $resultado = ($texto | Where-Object { $_ -match 'RESULT ' }) -join "`n"
    if (-not $ok) { $falhas += "$($b.Teste): o teste falhou" }
    foreach ($m in $b.Metricas) {
        if ($resultado -match $m.Padrao) {
            $valores[$m.Chave] = [double]::Parse($Matches[1], $inv)
        } else {
            $falhas += "$($m.Nome): sem resultado"
        }
    }
}

$anterior = $null
$arquivoAnterior = Join-Path $saida 'ultima.json'
if (Test-Path $arquivoAnterior) { $anterior = Get-Content -Raw -Encoding utf8 $arquivoAnterior | ConvertFrom-Json }

$acima = 0; $piorou = 0
$linhas = foreach ($b in $bateria) {
    foreach ($m in $b.Metricas) {
        if (-not $valores.Contains($m.Chave)) { continue }
        $v = $valores[$m.Chave]
        $antes = $null
        if ($anterior -and ($anterior.valores.PSObject.Properties.Name -contains $m.Chave)) { $antes = [double]$anterior.valores.($m.Chave) }
        $situacao = 'ok'
        if ($null -ne $m.Limite -and $v -gt $m.Limite) { $situacao = 'ACIMA DO LIMITE'; $acima++ }
        elseif ($null -ne $antes -and $v -gt $antes * $PIORA -and ($v - $antes) -gt $m.Folga) { $situacao = 'PIOROU'; $piorou++ }
        [pscustomobject]@{
            Medida   = $m.Nome
            Valor    = "$($v.ToString('0.###', $inv)) $($m.Unidade)"
            Limite   = $(if ($null -ne $m.Limite) { "$($m.Limite) $($m.Unidade)" } else { '-' })
            Anterior = $(if ($null -ne $antes) { "$($antes.ToString('0.###', $inv)) $($m.Unidade)" } else { '-' })
            Situacao = $situacao
        }
    }
}

$tabela = $linhas | Format-Table -AutoSize -Wrap | Out-String -Width 220
$commit = (& git -C $PSScriptRoot rev-parse --short HEAD 2>$null)
if (& git -C $PSScriptRoot status --porcelain 2>$null) { $commit = "$commit, com alterações ainda sem commit" }
$resumo = @("Bateria de medidas da 0.6, $($inicio.ToString('yyyy-MM-dd HH:mm')) (levou $([math]::Round(((Get-Date) - $inicio).TotalMinutes, 1)) min; código: $commit)", $tabela)
if ($script:pulados) { $resumo += 'Pulado:'; $resumo += $script:pulados | ForEach-Object { "  - $_" } }
if ($falhas) { $resumo += 'Falhas:'; $resumo += $falhas | ForEach-Object { "  - $_" } }
$resumo += "Acima do limite: $acima. Pioraram mais de 25%: $piorou. Medidas que falharam: $($falhas.Count)."
$resumo | Out-File -Append -Encoding utf8 $log
$resumo | Write-Host
Write-Host "Registro completo: $log"

# Só uma rodada completa e sem falhas vira a referência da próxima.
if (-not $falhas) {
    [pscustomobject]@{ data = $inicio.ToString('s'); valores = $valores } | ConvertTo-Json | Out-File -Encoding utf8 $arquivoAnterior
}
$historico = Join-Path $saida 'historico.csv'
if (-not (Test-Path $historico)) { 'data;medida;valor' | Out-File -Encoding utf8 $historico }
foreach ($k in $valores.Keys) { "$($inicio.ToString('s'));$k;$($valores[$k].ToString($inv))" | Out-File -Append -Encoding utf8 $historico }

if ($acima -or $falhas) { exit 1 }
if ($piorou) { exit 2 }
exit 0
