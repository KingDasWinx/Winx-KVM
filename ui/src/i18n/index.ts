import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';

import enCommon from './locales/en/common.json';
import enSettings from './locales/en/settings.json';
import ptBRCommon from './locales/pt-BR/common.json';
import ptBRSettings from './locales/pt-BR/settings.json';

export const SUPPORTED_LOCALES = ['en', 'pt-BR'] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { common: enCommon, settings: enSettings },
      'pt-BR': { common: ptBRCommon, settings: ptBRSettings },
    },
    fallbackLng: 'en',
    supportedLngs: SUPPORTED_LOCALES as unknown as string[],
    ns: ['common', 'settings'],
    defaultNS: 'common',
    interpolation: {
      escapeValue: false,
    },
    detection: {
      order: ['localStorage', 'navigator'],
      lookupLocalStorage: 'winx.locale',
      caches: ['localStorage'],
    },
  });

export default i18n;
