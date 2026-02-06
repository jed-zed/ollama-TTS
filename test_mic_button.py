"""
测试字幕窗口麦克风按钮 - 检查 toast 提示
"""
from playwright.sync_api import sync_playwright
import time

with sync_playwright() as p:
    browser = p.chromium.launch(headless=False)
    page = browser.new_page()
    
    # 监听控制台日志
    page.on("console", lambda msg: print(f"[Console] {msg.type}: {msg.text}"))
    
    # 打开字幕窗口页面 - 使用 HashRouter 格式
    print("正在打开页面...")
    page.goto('http://localhost:5173/#/overlay', timeout=60000)
    
    # 等待页面加载
    print("等待页面加载...")
    time.sleep(3)
    
    # 截图查看当前状态
    page.screenshot(path='I:/BaiduNetdiskDownload/ai/react/test_overlay_1_before.png', full_page=True)
    print("截图1已保存: test_overlay_1_before.png")
    
    # 查找麦克风按钮
    mic_button = page.locator('button:has(svg.lucide-mic), button:has(svg.lucide-mic-off)')
    mic_count = mic_button.count()
    print(f"找到 {mic_count} 个麦克风按钮")
    
    if mic_count > 0:
        btn = mic_button.first
        is_disabled = btn.is_disabled()
        title = btn.get_attribute('title')
        print(f"麦克风按钮: title={title}, disabled={is_disabled}")
        
        print("点击麦克风按钮...")
        btn.click()
        
        # 等待 toast 出现
        time.sleep(1)
        page.screenshot(path='I:/BaiduNetdiskDownload/ai/react/test_overlay_2_after_click.png', full_page=True)
        print("截图2已保存: test_overlay_2_after_click.png")
        
        # 检查是否有 toast
        toasts = page.locator('[data-sonner-toast], .sonner-toast, [role="status"]').all()
        print(f"找到 {len(toasts)} 个 toast 元素")
        
        for i, toast in enumerate(toasts):
            try:
                text = toast.inner_text()
                print(f"Toast {i}: {text}")
            except:
                pass
        
        # 再等待一下
        time.sleep(2)
        page.screenshot(path='I:/BaiduNetdiskDownload/ai/react/test_overlay_3_final.png', full_page=True)
        print("截图3已保存: test_overlay_3_final.png")
    
    browser.close()
    print("测试完成")
