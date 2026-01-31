import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { BrowserRouter, Routes, Route, Navigate } from "react-router-dom";
import AppLayout from "@/components/AppLayout";
import Service from "@/pages/Service";
import Subtitles from "@/pages/Subtitles";
import Voice from "@/pages/Voice";
import Logs from "@/pages/Logs";
import NotFound from "@/pages/NotFound";

const queryClient = new QueryClient();

const App = () => (
  <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <AppLayout>
           <Routes>
             <Route path="/" element={<Service />} />
             <Route path="/index.html" element={<Navigate to="/" replace />} />
             <Route path="/subtitles" element={<Subtitles />} />
             <Route path="/voice" element={<Voice />} />
             <Route path="/logs" element={<Logs />} />
             <Route path="*" element={<NotFound />} />
           </Routes>
        </AppLayout>
      </BrowserRouter>
  </QueryClientProvider>
);

export default App;