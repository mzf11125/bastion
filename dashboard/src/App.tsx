import { BrowserRouter, Routes, Route, Navigate, useNavigate } from 'react-router-dom';
import { useMemo } from 'react';
import { ConnectionProvider, WalletProvider } from '@solana/wallet-adapter-react';
import { SolflareWalletAdapter } from '@solana/wallet-adapter-solflare';
import { WalletModalProvider } from '@solana/wallet-adapter-react-ui';
import { ThemeProvider } from './context/ThemeContext';
import Landing from './pages/Landing';
import Dashboard from './pages/Dashboard';
import '@solana/wallet-adapter-react-ui/styles.css';

const DEVNET_ENDPOINT = 'https://api.devnet.solana.com';

function AppRoutes() {
  const wallets = useMemo(() => [new SolflareWalletAdapter()], []);

  return (
    <ConnectionProvider endpoint={DEVNET_ENDPOINT}>
      <WalletProvider wallets={wallets} autoConnect>
        <WalletModalProvider>
          <Routes>
            <Route path="/"          element={<Landing />} />
            <Route path="/dashboard" element={<Dashboard />} />
            <Route path="*"          element={<Navigate to="/" replace />} />
          </Routes>
        </WalletModalProvider>
      </WalletProvider>
    </ConnectionProvider>
  );
}

export function App() {
  return (
    <ThemeProvider>
      <BrowserRouter>
        <AppRoutes />
      </BrowserRouter>
    </ThemeProvider>
  );
}
