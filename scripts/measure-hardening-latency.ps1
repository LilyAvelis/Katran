param(
    [ValidateSet('debug','release')][string]$Profile = 'debug',
    [ValidateSet(5,40)][int]$PollMilliseconds = 5,
    [int]$Samples = 30,
    [string]$TargetDirectory = 'target/build'
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$deps = Join-Path $root "$TargetDirectory/$Profile/deps"
$work = Join-Path ([IO.Path]::GetTempPath()) ('katran-latency-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
try {
    $common = Get-Content -Raw -LiteralPath (Join-Path $root 'tests/common/source_api.rs')
    $common = $common.Replace('Duration::from_millis(5)', "Duration::from_millis($PollMilliseconds)")
    if ($PollMilliseconds -eq 40) { $common = $common.Replace('limit: 8', 'limit: 1') }
    Set-Content -LiteralPath (Join-Path $work 'source_api.rs') -Value $common -Encoding utf8
    $original = Get-Content -Raw -LiteralPath (Join-Path $root 'tests/latency.rs')
    $header = $original.Substring(0, $original.IndexOf('#[test]')).Replace('common/source_api.rs','source_api.rs')
    $native = $original.Substring($original.IndexOf('    #[cfg(windows)]'))
    $native = $native.Replace('                20,', "                $Samples,")
    $setup = @'
#[test]
fn native_latency_probe() {
    println!("BOUNDARY synthetic file write start -> trusted source commit -> BrowseShelf result; no renderer or Clipboard content");
    let temp = tempfile::tempdir().unwrap();
    let data = temp.path().join("data");
    let mut history = Service::open(&data, QUOTA).unwrap();
    for i in 0..1100 {
        call(&mut history, Command::IngestBytes { name:format!("synthetic-{i}.png"), bytes:vec![1,2,3], source:"clipboard_image".into() });
        if i == 99 || i == 1099 {
            measure(&format!("fixed_page_100_history_{}", i+1),30, |_| {
                let page=call(&mut history,Command::BrowseShelf {query:String::new(),limit:100,after_id:None});
                assert_eq!(page["events"].as_array().unwrap().len(),100);
            });
        }
    }
    let small_path = temp.path().join("small.png");
    image::RgbaImage::new(32,32).save(&small_path).unwrap();
    let small_png = std::fs::read(&small_path).unwrap();
    let big_path = temp.path().join("large.png");
    image::RgbaImage::from_fn(1920,1080, |x,y| { let n=x.wrapping_mul(1664525).wrapping_add(y.wrapping_mul(1013904223)); image::Rgba([n as u8,(n>>8) as u8,(n>>16) as u8,255]) }).save(&big_path).unwrap();
    let png = std::fs::read(&big_path).unwrap();
    println!("FIXTURE history=1100 small_png_bytes={} large_png_bytes={}",small_png.len(),png.len());
'@
    Set-Content -LiteralPath (Join-Path $work 'probe.rs') -Value ($header + $setup + $native) -Encoding utf8
    $arguments = @('--edition=2024','--test',(Join-Path $work 'probe.rs'),'-L',"dependency=$deps",'-o',(Join-Path $work 'probe.exe'))
    if ($Profile -eq 'release') { $arguments += @('-C','opt-level=3','-C','debug-assertions=no') }
    foreach ($crate in @('katran','image','serde_json','tempfile')) {
        $library = Get-ChildItem -LiteralPath $deps -Filter "lib$crate-*.rlib" | Sort-Object LastWriteTime -Descending | Select-Object -First 1
        if (!$library) { throw "Missing $crate library: build the selected profile first" }
        Write-Output "LIBRARY $crate=$($library.Name) timestamp=$($library.LastWriteTime.ToString('o'))"
        $arguments += @('--extern',"$crate=$($library.FullName)")
    }
    Write-Output "PROBE profile=$Profile poll_ms=$PollMilliseconds samples=$Samples; poll limit=1 for 40ms, otherwise 8; newest built libraries, not installed runtime"
    & rustc @arguments
    if ($LASTEXITCODE -ne 0) { throw 'Probe compilation failed' }
    & (Join-Path $work 'probe.exe') --nocapture
    if ($LASTEXITCODE -ne 0) { throw 'Probe failed' }
} finally {
    # Only the fresh GUID directory created above is removed.
    if ((Split-Path -Parent $work) -ne [IO.Path]::GetTempPath().TrimEnd('\')) { throw 'Unexpected scratch path' }
    Remove-Item -LiteralPath $work -Recurse -Force
}
