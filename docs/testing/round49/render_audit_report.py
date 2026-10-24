from pathlib import Path
from datetime import datetime
import html, re, json
root=Path(__file__).resolve().parents[3]
folder=Path(__file__).resolve().parent
report=folder/'round49-xingbu-wip-audit.md'
text=report.read_text(encoding='utf-8')
missing=[]
for target in re.findall(r'\]\(([^)]+)\)',text):
    target=target.strip('<>')
    if '://' in target: continue
    path=target.split('#',1)[0]
    if path and not (folder/path).resolve().exists(): missing.append(target)
assert not missing, missing
issues=[
 ('critical','high','合法负方向步长仍被额外拒绝','crates/mir-asset/src/frames.rs:77-87,102','Start20 Count6 Skip-8在八方向均不越界，Crystal按有符号公式读取，当前Rust整库拒绝。','按八方向实际范围与库长校验，补合法负stride和越界失败的区分断言。'),
 ('critical','high','最终证据身份和批次未闭环','docs/testing/monster-native/manifest.json / freeze.json / raw-manifest.json','52张PNG/crop真哈希匹配，但旧summary源hash、exe、batch与当前代码/新raw不同。','重建统一batch，记录最后源码、二进制、输入资源、命令与实际覆盖数；修复后补定向验证。'),
 ('critical','high','两份完整冻结JSON超1MB入库上限','docs/testing/monster-native/player-resource-freeze-before.json / player-resource-freeze.json','44,844,165与45,219,533字节；目录126,287,870字节。','完整逐文件哈希保留在target，小摘要入库；关键PNG显式选择。'),
 ('medium','med','session与scene资源来源可能不同','crates/mir-client/src/assets.rs:195','CLI解析--asset-mode但加载函数重新按环境变量/Auto发现。','复用已解析的ResolvedAssets/EntityLibraries，验证dev和pack-only release。'),
 ('medium','med','支持范围及R47冻结说明过期','crates/mir-render/src/entities.rs:1496 / docs/testing/attack-variants','Blend/Reverse/Effect及Sorting不全；attack PNG是真重跑但旧actual_index字段并不证明实际原生玩家索引。','明确PARTIAL，保全旧批次或记录更新决策，同步README/CHANGELOG/freeze与真实索引。')
]
def block(row):
    cls,severity,title,loc,description,suggestion=row
    q=html.escape
    return f'<div class="issue {cls}"><div class="issue-header"><span class="severity-badge {cls}">{q(severity)}</span><strong>{q(title)}</strong></div><div class="issue-location">{q(loc)}</div><div class="issue-description">{q(description)}</div><div class="issue-suggestion"><strong>建议：</strong>{q(suggestion)}</div></div>'
good=['Count×Interval生命周期、-OffSetMove符号及DrawY主锚已有明确源文和断言。','CLI恒空表及ignored缺素材空过已在并行修复中关闭源码问题。','25张攻击PNG与52张原生PNG/crop均通过独立真实哈希核验，未见造假。']
q=html.escape
values={
'report_title':q('R49 WIP 御史台只读审计：BLOCK'),
'review_timestamp':q(datetime.now().astimezone().isoformat(timespec='seconds')),
 'total_score':'6.3','total_score_percent':'63%',
 'summary_description':q('当前WIP不可整体提交。三个high尚未关闭；已修项与未修项分列。评分是审查定性参考，不代替任何验收门禁。'),
 'critical_count':'3','medium_count':'2','minor_count':'0','good_points_count':'3',
 'code_quality_score':'6','security_score':'8','performance_score':'5','maintainability_score':'6',
 'critical_issues':''.join(block(i) for i in issues if i[0]=='critical'),
 'medium_issues':''.join(block(i) for i in issues if i[0]=='medium'),
 'minor_issues':'<p>没有将命名或排版问题列为阻塞。</p>',
 'good_points':''.join('<div class="good-point">'+q(x)+'</div>' for x in good),
 'improvement_suggestions':'<p><a href="round49-xingbu-wip-audit.md">打开权威Markdown报告</a>，包含A–E与精确提交清单。</p><details><summary>完整Markdown报告快照</summary><pre style="white-space:pre-wrap">'+q(text)+'</pre></details>'
}
template=Path('C:/Users/diana/.dsh/skills/code-review-7d/templates/report-template.html').read_text(encoding='utf-8')
for key,value in values.items(): template=template.replace('{{'+key+'}}',value)
assert '{{' not in template and '}}' not in template
out=folder/'code-review-report-round49.html'
out.write_text(template,encoding='utf-8')
validation={'report_bytes':report.stat().st_size,'html_bytes':out.stat().st_size,'local_markdown_links_checked':len(re.findall(r'\]\(([^)]+)\)',text)),'missing_local_links':missing,'unfilled_placeholders':False,'all_audit_files_utf8':True}
for p in folder.iterdir():
    if p.is_file() and p.suffix in ('.md','.json','.py','.html','.sh'):
        p.read_text(encoding='utf-8-sig')
        assert p.stat().st_size < 1_000_000, str(p)
(folder/'audit-report-validation.json').write_text(json.dumps(validation,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(validation,ensure_ascii=False,indent=2))
