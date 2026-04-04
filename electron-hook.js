const { spawn } = require('child_process');
const path = require('path');

// Путь к скомпилированному бинарнику
// Для Electron: path.join(process.resourcesPath, 'global_mouse_hook')
const binaryPath = path.join(__dirname, 'target', 'release', 'global_mouse_hook');

console.log('🚀 Запускаем Rust-хук:', binaryPath);

const hook = spawn(binaryPath, {
  stdio: ['ignore', 'pipe', 'pipe'], // stdin закрыт, stdout/stderr пайпы
  env: { ...process.env, RUST_LOG: 'error' }
});

// Читаем JSON из stdout
hook.stdout.on('data', (chunk) => {
  const lines = chunk.toString().split('\n').filter(Boolean);
  for (const line of lines) {
    try {
      const event = JSON.parse(line.trim());
      console.log('⌨️ Electron Event:', event);
      // Здесь отправляйте в renderer через ipcMain
      // mainWindow.webContents.send('keyboard-event', event);
    } catch (e) {
      // Игнорируем мусорные строки
    }
  }
});

// Логи ошибок Rust
hook.stderr.on('data', (data) => {
  process.stderr.write(`🔴 Rust: ${data}`);
});

hook.on('error', (err) => console.error('❌ Не удалось запустить хук:', err));
hook.on('close', (code) => console.log(`🛑 Хук завершён с кодом ${code}`));

// Грейсфул-стоп
process.on('SIGINT', () => {
  console.log('\n🛑 Остановка хука...');
  hook.kill('SIGTERM');
  process.exit(0);
});