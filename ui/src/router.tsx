import { createBrowserRouter } from 'react-router-dom';

import { AppLayout } from './layouts/AppLayout';
import { HomePage } from './pages/HomePage';
import { SettingsPage } from './pages/SettingsPage';

export const router = createBrowserRouter([
  {
    element: <AppLayout />,
    children: [
      { index: true, element: <HomePage /> },
      { path: 'settings', element: <SettingsPage /> },
    ],
  },
]);
