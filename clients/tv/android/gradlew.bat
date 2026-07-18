@ECHO OFF
SET "PROJECT_DIR=%~dp0."
CALL "%~dp0..\..\mobile\android\gradlew.bat" -p "%PROJECT_DIR%" %*
EXIT /B %ERRORLEVEL%
