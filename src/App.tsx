/**
 * 应用入口
 * 使用 React.lazy 实现路由级代码分割，预加载关键页面
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { HashRouter, Routes, Route } from "react-router-dom";
import { lazy, Suspense, useEffect } from "react";
import ErrorBoundary from "@/components/ErrorBoundary";
import AppLayout from "@/components/AppLayout";
import FloatingSubtitle from "@/components/FloatingSubtitle";
import { useConfigStore } from "@/stores/configStore";

// 懒加载页面组件
const Service = lazy(() => import("@/pages/Service"));
const Subtitles = lazy(() => import("@/pages/Subtitles"));
const Voice = lazy(() => import("@/pages/Voice"));
const Logs = lazy(() => import("@/pages/Logs"));
const Memory = lazy(() => import("@/pages/Memory"));
const NotFound = lazy(() => import("@/pages/NotFound"));
const Overlay = lazy(() => import("@/pages/Overlay"));

// 预加载关键页面（在空闲时间加载）
const preloadPages = () => {
  // 使用 requestIdleCallback 在浏览器空闲时预加载
  const preload = () => {
    import("@/pages/Subtitles");
    import("@/pages/Voice");
  };

  if ('requestIdleCallback' in window) {
    (window as Window & { requestIdleCallback: (cb: () => void) => void }).requestIdleCallback(preload);
  } else {
    // 降级方案：延迟 2 秒后预加载
    setTimeout(preload, 2000);
  }
};

// React Query 客户端配置
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30 * 1000, // 30 秒内数据视为新鲜
      gcTime: 5 * 60 * 1000, // 5 分钟后垃圾回收
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

// 页面加载占位符
const PageLoader = () => (
  <div className="flex items-center justify-center h-full min-h-[200px]">
    <div className="w-6 h-6 border-2 border-primary border-t-transparent rounded-full animate-spin" />
  </div>
);

// 预加载触发组件 + 配置加载
const PreloadTrigger = () => {
  const loadConfig = useConfigStore((state) => state.loadConfig);
  
  useEffect(() => {
    loadConfig();
    preloadPages();
  }, [loadConfig]);
  return null;
};

const App = () => (
  <ErrorBoundary>
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <PreloadTrigger />
        <Routes>
          <Route path="/overlay" element={<Suspense fallback={<PageLoader />}><Overlay /></Suspense>} />
          <Route path="/" element={<AppLayout />}>
            <Route index element={<ErrorBoundary><Suspense fallback={<PageLoader />}><Service /></Suspense></ErrorBoundary>} />
            <Route path="subtitles" element={<ErrorBoundary><Suspense fallback={<PageLoader />}><Subtitles /></Suspense></ErrorBoundary>} />
            <Route path="voice" element={<ErrorBoundary><Suspense fallback={<PageLoader />}><Voice /></Suspense></ErrorBoundary>} />
            <Route path="memory" element={<ErrorBoundary><Suspense fallback={<PageLoader />}><Memory /></Suspense></ErrorBoundary>} />
            <Route path="logs" element={<ErrorBoundary><Suspense fallback={<PageLoader />}><Logs /></Suspense></ErrorBoundary>} />
            <Route path="*" element={<Suspense fallback={<PageLoader />}><NotFound /></Suspense>} />
          </Route>
        </Routes>
        <FloatingSubtitle />
      </HashRouter>
    </QueryClientProvider>
  </ErrorBoundary>
);

export default App;
