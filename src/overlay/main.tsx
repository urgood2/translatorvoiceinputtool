import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import '../index.css';

function OverlayApp() {
  return (
    <div className="min-h-screen pointer-events-none flex items-end justify-center p-6">
      <div className="rounded-lg border border-blue-400/50 bg-black/70 px-4 py-2 text-sm text-blue-100 shadow-lg">
        Overlay ready
      </div>
    </div>
  );
}

createRoot(document.getElementById('overlay-root')!).render(
  <StrictMode>
    <OverlayApp />
  </StrictMode>,
);
