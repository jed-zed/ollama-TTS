import React from 'react';
import { Rocket, Box, Network, Fingerprint, Copy, RefreshCw, Layers } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardDescription, CardHeader, CardTitle, CardFooter } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Separator } from '@/components/ui/separator';
import { Textarea } from '@/components/ui/textarea';

const Service = () => {
  return (
    <div className="space-y-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
      <div className="flex justify-between items-end">
        <div>
          <h2 className="text-3xl font-bold bg-gradient-to-r from-pink-500 to-purple-600 bg-clip-text text-transparent">服务配置</h2>
          <p className="text-muted-foreground mt-2">配置您的 Ollama API 和模型参数以适配 OpenAI 格式。</p>
        </div>
        <div className="flex gap-3">
          <Button variant="outline" className="shadow-sm border-primary/20 hover:bg-primary/5 text-primary">
            <RefreshCw className="mr-2 h-4 w-4" /> 测试连接
          </Button>
          <Button className="shadow-custom shadow-pink-500/20">
            <Rocket className="mr-2 h-4 w-4" /> 启动服务
          </Button>
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <Card className="glass border-none shadow-custom">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="p-2 rounded-lg bg-blue-100 text-blue-600">
                <Network className="h-5 w-5" />
              </div>
              <div>
                <CardTitle>API 连接</CardTitle>
                <CardDescription>设置 Ollama 的本地或远程接口地址</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
            <div className="space-y-2">
              <Label>API 预设</Label>
              <Select defaultValue="custom">
                <SelectTrigger>
                  <SelectValue placeholder="选择预设" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="custom">自定义 (Custom)</SelectItem>
                  <SelectItem value="local">本地默认 (Localhost)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            
            <div className="space-y-2">
              <Label>API URL</Label>
              <Input defaultValue="http://127.0.0.1:11434/v1" className="bg-white/50" />
            </div>

            <div className="space-y-2">
              <Label>API Key</Label>
              <div className="relative">
                <Input type="password" defaultValue="ollama" className="bg-white/50 pr-10" />
                <Fingerprint className="absolute right-3 top-2.5 h-4 w-4 text-muted-foreground" />
              </div>
            </div>
          </CardContent>
        </Card>

        <Card className="glass border-none shadow-custom">
          <CardHeader>
            <div className="flex items-center gap-3">
              <div className="p-2 rounded-lg bg-pink-100 text-pink-600">
                <Box className="h-5 w-5" />
              </div>
              <div>
                <CardTitle>模型设置</CardTitle>
                <CardDescription>指定用于推理的 LLM 模型及系统提示词</CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-4">
             <div className="space-y-2">
              <Label>模型名称 (Model Name)</Label>
              <div className="flex gap-2">
                <Input defaultValue="llama3:8b" className="bg-white/50" />
                <Button size="icon" variant="outline" className="shrink-0">
                   <Copy className="h-4 w-4" />
                </Button>
              </div>
            </div>

            <div className="space-y-2">
              <Label>系统提示词 (System Prompt)</Label>
              <Textarea 
                placeholder="在此输入系统角色设定..." 
                className="bg-white/50 min-h-[100px] resize-none"
                defaultValue="你是一个乐于助人的AI助手，请以简洁的风格回答问题。"
              />
            </div>
          </CardContent>
          <CardFooter className="bg-muted/30 pt-4 rounded-b-xl">
             <div className="text-xs text-muted-foreground flex gap-2 w-full justify-between items-center">
                <span>附加选项:</span>
                <div className="flex gap-2">
                  <Button variant="ghost" size="sm" className="h-6 text-xs hover:bg-white/80">+ preview 参数</Button>
                  <Button variant="ghost" size="sm" className="h-6 text-xs hover:bg-white/80">重置配置</Button>
                </div>
             </div>
          </CardFooter>
        </Card>
      </div>

      <Card className="border-dashed border-2 bg-transparent shadow-none">
        <CardContent className="flex flex-col items-center justify-center py-10 text-muted-foreground">
           <Layers className="h-12 w-12 text-muted-foreground/30 mb-4" />
           <p className="font-medium">扩展连接配置</p>
           <p className="text-sm mt-1">如需配置更多中转节点或负载均衡，请启用高级模式</p>
           <Button variant="link" className="text-primary mt-2">启用高级设置</Button>
        </CardContent>
      </Card>
    </div>
  );
};

export default Service;