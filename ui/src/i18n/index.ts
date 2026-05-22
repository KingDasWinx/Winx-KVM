import i18n from 'i18next';
import LanguageDetector from 'i18next-browser-languagedetector';
import { initReactI18next } from 'react-i18next';

import enCommon from './locales/en/common.json';
import enLab from './locales/en/lab.json';
import enSettings from './locales/en/settings.json';
import enWorkspace from './locales/en/workspace.json';
import ptBRCommon from './locales/pt-BR/common.json';
import ptBRLab from './locales/pt-BR/lab.json';
import ptBRSettings from './locales/pt-BR/settings.json';
import ptBRWorkspace from './locales/pt-BR/workspace.json';

export const SUPPORTED_LOCALES = ['en', 'pt-BR'] as const;
export type SupportedLocale = (typeof SUPPORTED_LOCALES)[number];

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { common: enCommon, lab: enLab, settings: enSettings, workspace: enWorkspace },
      'pt-BR': { common: ptBRCommon, lab: ptBRLab, settings: ptBRSettings, workspace: ptBRWorkspace },
    },
    fallbackLng: 'en',
    supportedLngs: SUPPORTED_LOCALES as unknown as string[],
    ns: ['common', 'lab', 'settings', 'workspace'],
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
