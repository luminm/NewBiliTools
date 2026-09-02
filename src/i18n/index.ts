import { createI18n } from 'vue-i18n';
import zhCN from './locales/zh-CN.json';

export const locales = [
  { id: 'zh-CN', name: '简体中文 🇨🇳', msg: zhCN },
];

export default createI18n({
  legacy: false,
  fallbackLocale: 'zh-CN',
  messages: Object.fromEntries(locales.map((loc) => [loc.id, loc.msg])),
});
