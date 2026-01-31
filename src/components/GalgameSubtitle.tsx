import React from 'react';

interface GalgameSubtitleProps {
  text?: string;
  name?: string;
  className?: string;
}

const GalgameSubtitle = ({ 
  text = "哈......你这家伙，真是有意思呢。竟然能找到这里来，看来我也不能小看你了。", 
  name = "神秘少女",
  className
}: GalgameSubtitleProps) => {
  return (
    <div className={`relative w-full max-w-3xl mx-auto group cursor-default select-none ${className}`}>
        {/* Name Tag */}
        <div className="absolute -top-5 left-8 z-20">
            <div className="bg-gradient-to-r from-pink-500 to-rose-400 text-white font-bold px-6 py-1.5 rounded-t-xl rounded-br-xl shadow-lg border-2 border-white/50 flex items-center gap-2 transform transition-transform group-hover:-translate-y-1">
                <span className="text-sm tracking-widest">{name}</span>
                <div className="w-2 h-2 rounded-full bg-white animate-pulse"></div>
            </div>
        </div>

        {/* Dialog Box Container */}
        <div className="relative bg-white/90 backdrop-blur-xl border-2 border-pink-200 rounded-2xl shadow-2xl p-6 pt-8 min-h-[140px] overflow-hidden">
            {/* Decorative Background Pattern */}
            <div className="absolute top-0 right-0 p-4 opacity-10">
               <svg width="100" height="100" viewBox="0 0 100 100" fill="none" xmlns="http://www.w3.org/2000/svg">
                   <path d="M50 0C50 0 50 50 100 50C50 50 50 100 50 100C50 100 50 50 0 50C50 50 50 0 50 0Z" fill="#EC4899"/>
               </svg>
            </div>
            <div className="absolute bottom-2 left-2 w-16 h-1 w-1/3 bg-gradient-to-r from-pink-200 to-transparent rounded-full"></div>

            {/* Content Text */}
            <p className="text-lg text-slate-700 leading-relaxed font-medium font-sans relative z-10">
                {text}
                <span className="inline-block w-2 h-4 ml-1 bg-pink-400 animate-pulse align-middle"></span>
            </p>

            {/* Next Message Triangle */}
            <div className="absolute bottom-4 right-6 animate-bounce">
                <div className="w-0 h-0 border-l-[8px] border-l-transparent border-r-[8px] border-r-transparent border-t-[12px] border-t-pink-400 opacity-80"></div>
            </div>
        </div>
        
        {/* Decorative corner accents */}
        <div className="absolute -bottom-1 -left-1 w-4 h-4 border-b-2 border-l-2 border-pink-400 rounded-bl-lg"></div>
        <div className="absolute -top-1 -right-1 w-4 h-4 border-t-2 border-r-2 border-pink-400 rounded-tr-lg"></div>
    </div>
  );
};

export default GalgameSubtitle;