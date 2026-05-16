import { createTheme, type MantineColorsTuple } from '@mantine/core';

const brand: MantineColorsTuple = [
  '#e6f4ff',
  '#cce4ff',
  '#99c8ff',
  '#66adff',
  '#3392ff',
  '#0078d6',
  '#0060ac',
  '#004883',
  '#003059',
  '#001830',
];

export const winxTheme = createTheme({
  primaryColor: 'brand',
  defaultRadius: 'md',
  fontFamily:
    'Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Oxygen, Ubuntu, sans-serif',
  fontFamilyMonospace: 'JetBrains Mono, Cascadia Code, Consolas, monospace',
  colors: {
    brand,
  },
  headings: {
    fontWeight: '600',
  },
});
