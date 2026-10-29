import './index.css';
import { QueryClientProvider } from '@tanstack/react-query';
import { StrictMode, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { BrowserRouter } from 'react-router';
import { Toaster } from '@/components/ui/sonner';
import { TooltipProvider } from '@/components/ui/tooltip';
import { App } from './App.tsx';
import { createQueryClient } from './lib/api.ts';
import { I18nProvider, isRtl, useLanguage } from './lib/i18n.tsx';
import { ThemeProvider } from './lib/theme.tsx';

function Toasts() {
  const rtl = isRtl(useLanguage().language);
  return <Toaster position={rtl ? 'bottom-left' : 'bottom-right'} dir={rtl ? 'rtl' : 'ltr'} />;
}

function Providers() {
  const [queryClient] = useState(createQueryClient);
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <I18nProvider>
          <TooltipProvider delayDuration={300}>
            <BrowserRouter>
              <App />
            </BrowserRouter>
            <Toasts />
          </TooltipProvider>
        </I18nProvider>
      </QueryClientProvider>
    </ThemeProvider>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Providers />
  </StrictMode>,
);
