// napi-rs передаёт ТОЛЬКО ОДИН аргумент. (err, event) здесь не работает.
import { startGlobalKeyboardHook, stopGlobalKeyboardHook } from './global-mouse-hook.linux-x64-gnu.node';

console.log('🔌 Подключаем хук клавиатуры...');

startGlobalKeyboardHook((raw) => {
  try {
    const event = typeof raw === 'string' ? JSON.parse(raw) : raw;
    if (event === null || event === undefined) return;
    console.log('⌨️ Key:', event);
  } catch (err) {
    console.error('⚠️ Ошибка парсинга:', err.message);
  }
});

console.log('🟢 Хук активен. Нажмите любые клавиши. Ctrl+C для выхода.');
process.stdin.resume();

process.on('SIGINT', () => {
  console.log('\n🛑 Остановка...');
  try { stopGlobalKeyboardHook(); } catch (_) {}
  process.exit(0);
});