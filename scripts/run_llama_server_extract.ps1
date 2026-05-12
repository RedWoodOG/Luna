$ErrorActionPreference = "Stop"

$prompt = [Console]::In.ReadToEnd()

$body = @{
    messages = @(
        @{
            role = "user"
            content = $prompt
        }
    )
    temperature = 0
    top_p = 1
    seed = 42
    max_tokens = 768
    stream = $false
} | ConvertTo-Json -Depth 10

$bodyBytes = [System.Text.Encoding]::UTF8.GetBytes($body)

$response = Invoke-RestMethod `
    -Uri "http://127.0.0.1:8181/v1/chat/completions" `
    -Method Post `
    -ContentType "application/json; charset=utf-8" `
    -Body $bodyBytes

$content = $response.choices[0].message.content

if ($null -eq $content) {
    throw "llama-server response did not include choices[0].message.content"
}

$stdout = [Console]::OpenStandardOutput()
$outputBytes = [System.Text.Encoding]::UTF8.GetBytes($content)
$stdout.Write($outputBytes, 0, $outputBytes.Length)
