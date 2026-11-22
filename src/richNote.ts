import {textParts} from './model';

type Options={
  parts:ReturnType<typeof textParts>;
  select:(start:number,end:number)=>void;
  edit:(value:string,start?:number)=>void;
};

export function richNote(node:HTMLElement,options:Options){
  let before:number|undefined,composing=false;
  function selection(){
    const sel=window.getSelection();
    if(!sel?.rangeCount)return;
    const range=sel.getRangeAt(0);
    if(!node.contains(range.commonAncestorContainer))return;
    const prefix=range.cloneRange();
    prefix.selectNodeContents(node);
    prefix.setEnd(range.startContainer,range.startOffset);
    const start=prefix.toString().length;
    return {range,start,end:start+range.toString().length};
  }
  function remember(){const s=selection();if(s)options.select(s.start,s.end);}
  function render(){
    const saved=selection();
    const spans=options.parts.map(part=>{
      const span=document.createElement('span');
      span.style.cssText=part.css;
      if(part.to){span.style.textDecoration='underline';span.style.color='var(--accent)';}
      span.append(document.createTextNode(part.text));
      return span;
    });
    node.replaceChildren(...spans);
    if(!saved)return;
    const point=(offset:number):[Node,number]=>{
      for(const span of spans){
        const text=span.firstChild as Text;
        if(offset<=text.length)return [text,offset];
        offset-=text.length;
      }
      return [node,node.childNodes.length];
    };
    const range=document.createRange();
    range.setStart(...point(saved.start));range.setEnd(...point(saved.end));
    const sel=window.getSelection()!;
    sel.removeAllRanges();sel.addRange(range);
  }
  function input(){if(!composing){options.edit(node.textContent??'',before);before=undefined;remember();}}
  function insert(text:string){
    const s=selection();if(!s)return;
    before=s.start;
    s.range.deleteContents();
    const leaf=document.createTextNode(text);
    s.range.insertNode(leaf);s.range.setStartAfter(leaf);s.range.collapse(true);
    const sel=window.getSelection()!;sel.removeAllRanges();sel.addRange(s.range);
    input();
  }
  const events=new AbortController(),opts={signal:events.signal};
  node.addEventListener('beforeinput',e=>{
    before=selection()?.start;
    if(e.inputType==='insertParagraph'||e.inputType==='insertLineBreak'){e.preventDefault();insert('\n');}
  },opts);
  node.addEventListener('input',input,opts);
  node.addEventListener('compositionstart',()=>{composing=true;},opts);
  node.addEventListener('compositionend',()=>{composing=false;input();},opts);
  node.addEventListener('paste',e=>{e.preventDefault();insert((e.clipboardData?.getData('text/plain')??'').replace(/\r\n?/g,'\n'));},opts);
  document.addEventListener('selectionchange',remember,opts);
  render();
  return {update(next:Options){options=next;if(!composing)render();},destroy(){events.abort();}};
}
