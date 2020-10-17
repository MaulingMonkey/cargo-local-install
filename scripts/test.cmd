@setlocal && pushd "%~dp0.."

cargo build --bin cargo-local-install
@set PATH=%~dp0..\target\debug;%PATH%

@cd "%~dp0..\test"
@rmdir "bin"                    2>NUL >NUL
@rmdir "empty\bin"              2>NUL >NUL
@rmdir "package-metadata\bin"   2>NUL >NUL
@rmdir "no-metadata\bin"        2>NUL >NUL

@mkdir "%~dp0..\test\empty" 2>NUL >NUL
@cd "%~dp0.."                       && call :expect-failure cargo-local-install || goto :die
@cd "%~dp0..\test\empty"            && call :expect-success cargo-local-install cargo-web --version "=0.6.26" --locked || goto :die
@cd "%~dp0..\test\empty"            && call :expect-success cargo-local-install || goto :die
@cd "%~dp0..\test\package-metadata" && call :expect-success cargo-local-install || goto :die
@cd "%~dp0..\test\no-metadata"      && call :expect-failure cargo-local-install || goto :die
:: Repeat cached
@cd "%~dp0..\test\empty"            && call :expect-success cargo-local-install || goto :die
@cd "%~dp0..\test\package-metadata" && call :expect-success cargo-local-install || goto :die
@cd "%~dp0..\test\no-metadata"      && call :expect-failure cargo-local-install || goto :die

@cd "%~dp0..\test"
@call :expect-version "empty\bin\cargo-web"            "0.6.26" || goto :die
@call :expect-version "bin\cargo-web"                  "0.6.26" || goto :die
@call :expect-version "bin\wasm-pack"                  "0.9.1"  || goto :die
@call :expect-version "package-metadata\bin\cargo-web" "0.6.26" || goto :die
@call :expect-failure "no-metadata\bin\cargo-web" "--version"   || goto :die
@call :expect-success "bin\test-local-package"                  || goto :die
@call :expect-failure "empty\bin\test-local-package"            || goto :die

:die
@echo.
@if ERRORLEVEL 1 (echo ERRORS) else (echo ALL TESTS PASS)
@endlocal && popd && exit /b %ERRORLEVEL%



:expect-success
%*
@if ERRORLEVEL 1 exit /b %ERRORLEVEL%
@exit /b 0

:expect-failure
%*
@if ERRORLEVEL 1 echo (error was expected)
@if ERRORLEVEL 1 exit /b 0
@exit /b 1

:expect-version
"%~1" --version > version.txt
@if ERRORLEVEL 1 echo Failed to run %1.&& del version.txt && exit /b 1
@type version.txt | findstr "%~2" >NUL
@if ERRORLEVEL 1 echo Expected version %2 for %1, got:&& type version.txt && del version.txt && exit /b 1
@del version.txt
@exit /b 0
