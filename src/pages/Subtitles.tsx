import React from 'react';
import { Eye, EyeOff, LayoutTemplate, Eraser, Settings, MousePointerClick, AppWindow } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import { Label } from '@/components/ui/label';
import { Slider } from '@/components/ui/slider';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import GalgameSubtitle from '@/components/GalgameSubtitle';

const Subtitles = () => {
  return (
    <div className="h-full flex flex-col gap-6 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div>
        <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">字幕视觉</h2>
        <p className="text-muted-foreground mt-2">自定义悬浮窗外观与行为，打造沉浸式阅读体验。</p>
      </div>

      <div className="grid grid-cols-12 gap-6 flex-1 min-h-0">
        {/* Controls Section */}
        <div className="col-span-4 flex flex-col gap-6 overflow-auto pr-2">
            <Card className="glass border-none shadow-custom">
              <CardHeader className="pb-3">
                <CardTitle className="text-lg flex items-center gap-2">
                  <AppWindow className="w-4 h-4 text-pink-500" />
                  窗口控制
                </CardTitle>
              </CardHeader>
              <CardContent className="grid grid-cols-2 gap-3">
                 <Button className="w-full bg-pink-50 text-pink-600 hover:bg-pink-100 border border-pink-200 shadow-sm">
                   <Eye className="w-4 h-4 mr-2" /> 显示字幕
                 </Button>
                 <Button variant="outline" className="w-full text-muted-foreground">
                   <EyeOff className="w-4 h-4 mr-2" /> 隐藏字幕
                 </Button>
                 <Button variant="outline" className="w-full text-muted-foreground">
                   <LayoutTemplate className="w-4 h-4 mr-2" /> 显示控件
                 </Button>
                 <Button variant="outline" className="w-full text-muted-foreground hover:text-destructive hover:bg-red-50 hover:border-red-200">
                   <Eraser className="w-4 h-4 mr-2" /> 清空内容
                 </Button>
              </CardContent>
            </Card>

            <Card className="glass border-none shadow-custom flex-1">
              <CardHeader className="pb-3">
                <CardTitle className="text-lg flex items-center gap-2">
                  <Settings className="w-4 h-4 text-blue-500" />
                  行为设置
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-6">
                 <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                       <Label className="text-base">鼠标穿透</Label>
                       <p className="text-xs text-muted-foreground">允许鼠标点击字幕后方的内容</p>
                    </div>
                    <Switch />
                 </div>
                 
                 <div className="flex items-center justify-between">
                    <div className="space-y-0.5">
                       <Label className="text-base">自动显示</Label>
                       <p className="text-xs text-muted-foreground">检测到新内容时自动弹出</p>
                    </div>
                    <Switch defaultChecked />
                 </div>

                 <div className="space-y-3">
                    <Label>不透明度 (Opacity)</Label>
                    <Slider defaultValue={[90]} max={100} step={1} className="py-2" />
                 </div>

                 <div className="space-y-3">
                    <Label>字体大小 (Font Size)</Label>
                    <Slider defaultValue={[18]} min={12} max={36} step={1} className="py-2" />
                 </div>
              </CardContent>
            </Card>
        </div>

        {/* Preview Section */}
        <div className="col-span-8 flex flex-col gap-6">
            <Card className="flex-1 glass border-none shadow-custom relative overflow-hidden flex flex-col">
               <div className="absolute inset-0 bg-gradient-to-br from-blue-50 to-pink-50 opacity-50 z-0"></div>
               {/* Background image replacement */}
               <div className="absolute inset-0 z-0 opacity-20" style={{backgroundImage: "url('https://images.unsplash.com/photo-1518099074173-7db8b65b6257?q=80&w=2000&auto=format&fit=crop')", backgroundSize: 'cover', backgroundPosition: 'center'}}></div>
               
               <CardHeader className="relative z-10 border-b border-white/20 bg-white/30 backdrop-blur-sm">
                 <div className="flex justify-between items-center">
                    <CardTitle>实时预览</CardTitle>
                    <Tabs defaultValue="galgame" className="w-[200px]">
                      <TabsList className="grid w-full grid-cols-2 h-8">
                        <TabsTrigger value="galgame" className="text-xs">Galgame</TabsTrigger>
                        <TabsTrigger value="standard" className="text-xs">标准模式</TabsTrigger>
                      </TabsList>
                    </Tabs>
                 </div>
               </CardHeader>
               
               <CardContent className="relative z-10 flex-1 flex flex-col items-center justify-end pb-12 px-12">
                  <div className="w-full text-center mb-8 text-muted-foreground text-sm font-medium bg-white/40 px-3 py-1 rounded-full inline-block backdrop-blur-sm">
                    预览区域 - 背景图仅供参考
                  </div>
                  
                  {/* The Galgame Subtitle Component */}
                  <GalgameSubtitle />
                  
               </CardContent>
            </Card>
        </div>
      </div>
    </div>
  );
};

export default Subtitles;