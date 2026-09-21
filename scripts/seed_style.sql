-- =============================================================================
-- Fashion AI Platform — Phase 1B T-012 seed data
-- File: seed_style.sql (56 rows)
-- Idempotent: rows are skipped when an item with the same identity exists.
-- Scope: attaches to the first org/dept found in the database.
-- =============================================================================

WITH ctx AS (
    SELECT d.org_id AS org_id, d.id AS dept_id
    FROM depts d
    ORDER BY d.created_at
    LIMIT 1
)
INSERT INTO styles
    (org_id, dept_id, name, description, silhouette, garment_type,
     key_features, suitable_seasons, target_audience)
SELECT c.org_id, c.dept_id, v.name, v.description, v.silhouette, v.garment_type,
       v.key_features, v.suitable_seasons, v.target_audience
FROM ctx c
CROSS JOIN (VALUES
    ('基础A字连衣裙','上半身合身、自腰部向下放开的经典连衣裙，对各种身材友好','A型','连衣裙',ARRAY['收腰','及膝','隐形拉链']::text[],ARRAY['spring','summer']::text[],ARRAY['都市女性','学生']::text[]),
    ('衬衫式连衣裙','衬衫结构加裙身，可系腰带切换直身与收腰造型','H型','连衣裙',ARRAY['翻领','门襟','配腰带']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['通勤女性']::text[]),
    ('茶歇裙','V领裹身设计的法式连衣裙，围裹系带、性感优雅','X型','连衣裙',ARRAY['V领','裹身','系带']::text[],ARRAY['spring','summer']::text[],ARRAY['轻熟女性']::text[]),
    ('针织直筒连衣裙','弹力针织贴身直筒，舒适与曲线兼具，叠穿百搭','H型','连衣裙',ARRAY['针织弹力','圆领','及膝']::text[],ARRAY['autumn','winter']::text[],ARRAY['通勤女性']::text[]),
    ('吊带睡裙风连衣裙','细吊带飘逸裙型，可单穿度假或内搭T恤日常','H型','连衣裙',ARRAY['细吊带','雪纺','大摆']::text[],ARRAY['summer']::text[],ARRAY['年轻女性']::text[]),
    ('旗袍式连衣裙','立领盘扣的改良连衣裙，东方韵味现代剪裁','S型','连衣裙',ARRAY['立领','盘扣','开衩']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['国风爱好者']::text[]),
    ('鱼尾礼服裙','修身至膝部后放开如鱼尾，仪式感与女人味十足','S型','连衣裙',ARRAY['包臀','下摆展开','及地']::text[],ARRAY['all-season']::text[],ARRAY['礼服场合']::text[]),
    ('娃娃裙','高腰线下摆宽松的可爱短款连衣裙，减龄俏皮','O型','连衣裙',ARRAY['高腰','宽松','泡泡袖']::text[],ARRAY['spring','summer']::text[],ARRAY['少女','学生']::text[]),
    ('衬衫背心两件套裙','衬衫与背心连衣裙的套装组合，层次通勤','H型','连衣裙',ARRAY['两件套','无袖背心裙','翻领衬衫']::text[],ARRAY['spring','autumn']::text[],ARRAY['通勤女性']::text[]),
    ('基础正装白衬衫','尖领正装剪裁的白衬衫，通勤与正式场合百搭','H型','衬衫',ARRAY['尖领','单排扣','长袖']::text[],ARRAY['all-season']::text[],ARRAY['通勤人群']::text[]),
    ('法式复古衬衫','小翻领、泡泡袖与珍珠扣的法式风格衬衫','X型','衬衫',ARRAY['泡泡袖','小方领','珍珠扣']::text[],ARRAY['spring','summer']::text[],ARRAY['轻熟女性']::text[]),
    ('牛津纺休闲衬衫','牛津纺纽扣领衬衫，休闲学院风','H型','衬衫',ARRAY['纽扣领','牛津纺','左胸袋']::text[],ARRAY['spring','autumn']::text[],ARRAY['学生','休闲男性']::text[]),
    ('立领中式衬衫','立领盘扣的中式上衣，国风日常可穿','H型','衬衫',ARRAY['立领','盘扣','棉麻']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['国风爱好者']::text[]),
    ('飘带蝴蝶结衬衫','领口系带蝴蝶结的柔美女衫，职场甜度平衡','X型','衬衫',ARRAY['飘带领','收腰','雪纺']::text[],ARRAY['spring','autumn']::text[],ARRAY['通勤女性']::text[]),
    ('oversize廓形衬衫','宽大廓形可作外搭的长衬衫，街头休闲','O型','衬衫',ARRAY['宽肩','落肩','长版']::text[],ARRAY['spring','summer']::text[],ARRAY['年轻女性']::text[]),
    ('牛仔衬衫','丹宁面料的衬衫，叠穿与休闲造型利器','H型','衬衫',ARRAY['牛仔','双胸袋','金属扣']::text[],ARRAY['all-season']::text[],ARRAY['年轻人群']::text[]),
    ('泡泡袖短衬衫','泡泡袖短款上衣，高腰搭配显比例','X型','衬衫',ARRAY['泡泡袖','短款 crop','方领']::text[],ARRAY['summer']::text[],ARRAY['少女']::text[]),
    ('基础圆领T恤','合身圆领短袖T恤，衣橱基础单品','H型','T恤',ARRAY['圆领','短袖','针织']::text[],ARRAY['spring','summer']::text[],ARRAY['全人群']::text[]),
    ('oversize落肩T恤','宽大落肩短袖，街头休闲造型','O型','T恤',ARRAY['落肩','宽松','长版']::text[],ARRAY['spring','summer']::text[],ARRAY['年轻人群']::text[]),
    ('Polo衫','翻领带门襟的针织休闲上衣，运动与通勤皆可','H型','T恤',ARRAY['翻领','2-3粒扣','珠地针织']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['休闲人群','高尔夫']::text[]),
    ('修身V领T恤','V领修身针织上衣，拉长颈部线条','X型','T恤',ARRAY['V领','修身','短袖']::text[],ARRAY['spring','summer']::text[],ARRAY['女性']::text[]),
    ('印花图案T恤','胸前印花的个性T恤，风格表达单品','H型','T恤',ARRAY['圆领','印花','短袖']::text[],ARRAY['spring','summer']::text[],ARRAY['学生','潮流人群']::text[]),
    ('双排扣战壕风衣','肩章、枪挡、腰带俱全的经典战壕风衣','X型','外套',ARRAY['双排扣','腰带','肩章']::text[],ARRAY['spring','autumn']::text[],ARRAY['都市人群']::text[]),
    ('单排扣通勤风衣','简洁单排扣中长风衣，轻熟干练','H型','外套',ARRAY['单排扣','暗门襟','中长款']::text[],ARRAY['spring','autumn']::text[],ARRAY['通勤人群']::text[]),
    ('双排扣海军大衣','大翻领双排扣短大衣，海军风格','H型','外套',ARRAY['双排扣','大翻领','肩章']::text[],ARRAY['autumn','winter']::text[],ARRAY['都市人群']::text[]),
    ('浴袍款羊绒大衣','无扣腰带裹身的羊绒大衣，慵懒高级','X型','外套',ARRAY['浴袍领','腰带','双面呢']::text[],ARRAY['autumn','winter']::text[],ARRAY['轻熟女性']::text[]),
    ('茧型大衣','肩部落肩、下摆收拢的O形大衣，可爱复古','O型','外套',ARRAY['落肩','廓形','大贴袋']::text[],ARRAY['winter']::text[],ARRAY['女性']::text[]),
    ('直筒H型大衣','直上直下的大衣，利落不挑人','H型','外套',ARRAY['平驳领','单排扣','直筒']::text[],ARRAY['autumn','winter']::text[],ARRAY['通勤人群']::text[]),
    ('机车皮衣','不对称拉链、腰带铆钉的短款皮衣','X型','外套',ARRAY['不对称门襟','拉链','铆钉']::text[],ARRAY['spring','autumn']::text[],ARRAY['酷感人群']::text[]),
    ('牛仔夹克','经典短款牛仔外套，四季叠穿','H型','外套',ARRAY['金属扣','双胸袋','短款']::text[],ARRAY['spring','autumn']::text[],ARRAY['全年龄']::text[]),
    ('连帽工装夹克','带帽多袋的工装短外套，户外机能','H型','外套',ARRAY['连帽','多口袋','魔术贴']::text[],ARRAY['spring','autumn']::text[],ARRAY['年轻男性']::text[]),
    ('平驳领商务西装','单排两粒扣平驳领正装西装，商务标配','X型','西装',ARRAY['平驳领','单排双扣','双开衩']::text[],ARRAY['all-season']::text[],ARRAY['商务男士']::text[]),
    ('戗驳领礼服西装','戗驳领塔士多礼服，晚宴正式场合','X型','西装',ARRAY['戗驳领','缎面镶边','一粒扣']::text[],ARRAY['all-season']::text[],ARRAY['礼服场合']::text[]),
    ('休闲棉麻西装','无结构或半衬的休闲西装，日常轻商务','H型','西装',ARRAY['无内衬','棉麻',' patch袋']::text[],ARRAY['spring','summer']::text[],ARRAY['年轻通勤']::text[]),
    ('oversize廓形西装','宽肩加长的潮流西装，街头风格','O型','西装',ARRAY['宽肩','落肩','加长']::text[],ARRAY['spring','autumn']::text[],ARRAY['潮流人群']::text[]),
    ('直筒西裤','高腰直筒正装裤，通勤修饰腿型','H型','裤装',ARRAY['高腰','直筒','烫迹线']::text[],ARRAY['all-season']::text[],ARRAY['通勤人群']::text[]),
    ('修身小脚裤','贴身收脚裤型，利落显高','X型','裤装',ARRAY['修身','窄脚','中腰']::text[],ARRAY['all-season']::text[],ARRAY['年轻人群']::text[]),
    ('阔腿裤','宽管垂坠裤型，舒适有气场','H型','裤装',ARRAY['高腰','宽管','垂坠']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['都市女性']::text[]),
    ('直筒牛仔裤','经典五袋直筒牛仔裤，百搭耐磨','H型','裤装',ARRAY['五袋','牛仔','拉链门襟']::text[],ARRAY['all-season']::text[],ARRAY['全人群']::text[]),
    ('工装多袋裤','立体多口袋的休闲工装裤，机能街头','H型','裤装',ARRAY['多口袋','帆布','束脚可选']::text[],ARRAY['spring','autumn']::text[],ARRAY['潮流男性']::text[]),
    ('运动束脚裤','针织抽绳束脚卫裤，运动休闲','H型','裤装',ARRAY['抽绳','罗纹束脚','针织']::text[],ARRAY['all-season']::text[],ARRAY['运动人群']::text[]),
    ('短裤','及膝休闲短裤，夏季基础下装','H型','裤装',ARRAY['短款','抽绳','斜纹']::text[],ARRAY['summer']::text[],ARRAY['年轻人群']::text[]),
    ('喇叭裤','膝部以下放开的复古喇叭裤型','X型','裤装',ARRAY['高腰','喇叭裤脚','修身']::text[],ARRAY['spring','autumn']::text[],ARRAY['复古爱好者']::text[]),
    ('A字半裙','腰部合体向下放开的经典半身裙','A型','裙装',ARRAY['及膝','隐形拉链','收腰']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['女性']::text[]),
    ('直筒铅笔裙','修身包臀直筒半裙，通勤女人味','X型','裙装',ARRAY['包臀','后开衩','及膝']::text[],ARRAY['all-season']::text[],ARRAY['通勤女性']::text[]),
    ('百褶裙','细密褶裥垂坠的半裙，学院与优雅兼具','A 型','裙装',ARRAY['百褶','高腰','及膝']::text[],ARRAY['spring','autumn']::text[],ARRAY['学生','轻熟女性']::text[]),
    ('牛仔半裙','丹宁材质的休闲半身裙','H型','裙装',ARRAY['牛仔','明线','前排扣']::text[],ARRAY['spring','summer','autumn']::text[],ARRAY['年轻女性']::text[]),
    ('大摆伞裙','高腰宽摆伞状裙型，复古浪漫','A型','裙装',ARRAY['高腰','大摆','及踝可选']::text[],ARRAY['spring','summer']::text[],ARRAY['复古女性']::text[]),
    ('针织迷笛裙','弹力针织迷笛长度半身裙，秋冬舒适','H型','裙装',ARRAY['针织','迷笛长度','弹力']::text[],ARRAY['autumn','winter']::text[],ARRAY['通勤女性']::text[]),
    ('短裙裤','裙裤合一的安全短裙，俏皮实用','A型','裙装',ARRAY['短裤内衬','高腰','短款']::text[],ARRAY['summer']::text[],ARRAY['少女','学生']::text[]),
    ('圆领基础毛衣','合身圆领针织毛衣，秋冬内搭外穿皆可','H型','针织',ARRAY['圆领','长袖','罗纹下摆']::text[],ARRAY['autumn','winter']::text[],ARRAY['全人群']::text[]),
    ('高领打底衫','贴身高领针织，叠穿保暖基础','H型','针织',ARRAY['高领','修身','细针织']::text[],ARRAY['autumn','winter']::text[],ARRAY['全人群']::text[]),
    ('针织开衫','纽扣开襟针织外套，温柔百搭','H型','针织',ARRAY['V领','单排扣','长袖']::text[],ARRAY['spring','autumn']::text[],ARRAY['女性']::text[]),
    ('oversize粗针织毛衣','粗棒针宽松毛衣，慵懒冬季造型','O型','针织',ARRAY['粗棒针','落肩','宽袖']::text[],ARRAY['winter']::text[],ARRAY['年轻女性']::text[]),
    ('针织马甲','无袖针织背心，学院叠穿单品','H型','针织',ARRAY['V领','绞花','无袖']::text[],ARRAY['spring','autumn']::text[],ARRAY['学生','通勤']::text[]),
    ('针织连衣裙','一体针织的修身连衣裙，秋冬温柔曲线','X型','针织',ARRAY['圆领','包臀','长袖']::text[],ARRAY['autumn','winter']::text[],ARRAY['女性']::text[])
) AS v(name, description, silhouette, garment_type, key_features, suitable_seasons, target_audience)
WHERE NOT EXISTS (
    SELECT 1 FROM styles st
    WHERE st.org_id = c.org_id AND st.name = v.name
);
