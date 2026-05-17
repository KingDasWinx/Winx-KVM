import '@mantine/core/styles.css';
import '@mantine/notifications/styles.css';
import '@fontsource/inter/400.css';
import '@fontsource/inter/600.css';
import '@fontsource/jetbrains-mono/400.css';

import React from 'react';
import ReactDOM from 'react-dom/client';
import { ColorSchemeScript, MantineProvider } from '@mantine/core';
import { Notifications } from '@mantine/notifications';

import { App } from './App';
import { winxTheme } from './theme';
import './i18n';

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('elemento #root não encontrado em index.html');

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <ColorSchemeScript defaultColorScheme="dark" />
    <MantineProvider theme={winxTheme} defaultColorScheme="dark">
      <Notifications position="top-right" zIndex={1000} />
      <App />
    </MantineProvider>
  </React.StrictMode>,
);
