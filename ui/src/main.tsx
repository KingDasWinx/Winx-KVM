import '@mantine/core/styles.css';

import React from 'react';
import ReactDOM from 'react-dom/client';
import { MantineProvider, ColorSchemeScript } from '@mantine/core';

import { App } from './App';
import { winxTheme } from './theme';
import './i18n';

const rootEl = document.getElementById('root');
if (!rootEl) throw new Error('elemento #root não encontrado em index.html');

ReactDOM.createRoot(rootEl).render(
  <React.StrictMode>
    <ColorSchemeScript defaultColorScheme="dark" />
    <MantineProvider theme={winxTheme} defaultColorScheme="dark">
      <App />
    </MantineProvider>
  </React.StrictMode>,
);
