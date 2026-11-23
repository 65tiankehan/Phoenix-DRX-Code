//! 物品图鉴：把 `templates/items.json` 全量物品按分类树摊平给前端。
//!
//! 只读：不碰存档。分类来自库里的 `_type: "Node"` 节点（120 个），
//! 物品沿 `_parent` 链上溯即可精确归位，不依赖任何写死的名称表。
//!
//! 「加入仓库」不走这里 —— 复用 `stash_save` 的 `AddItem`，保证备份与放置引擎同一条路径。

use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use tauri::AppHandle;

use crate::character::database_dir;
use crate::server::data_language;

/// 物品树根节点（库里叫 `Item`，12 个一级分类挂在它下面）
const ROOT: &str = "54009119af1c881c07000029";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCategory {
    pub id: String,
    pub name: String,
    /// 上级分类 id；根（ROOT 的直接子节点）为 None
    pub parent: Option<String>,
    /// 相对 ROOT 的层级：1 = 一级分类（组合物品/药品/钥匙…）
    pub depth: u8,
    /// 该分类子树下的物品数
    pub count: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogItem {
    pub tpl: String,
    pub name: String,
    pub short: String,
    /// 最贴近的叶子分类 id（前端按「选中的分类子树是否包含它」筛选）
    pub leaf: String,
    pub w: i64,
    pub h: i64,
    pub stack: i64,
    pub price: i64,
    pub rarity: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Catalog {
    pub categories: Vec<CatalogCategory>,
    pub items: Vec<CatalogItem>,
    /// 没有归到任何一级分类的物品数（库结构异常时用来提示，正常为 0）
    pub orphans: usize,
}

fn as_i64(v: Option<&Value>) -> i64 {
    match v {
        Some(Value::Number(n)) => n.as_i64().or_else(|| n.as_f64().map(|f| f as i64)).unwrap_or(0),
        Some(Value::String(s)) => s.trim().parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn name_of(names: &Value, key: &str) -> String {
    names
        .get(format!("{} Name", key))
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

/// 物品图鉴全量数据。库不可读时返回 None（前端显示「未选择服务器」）。
pub fn catalog(app: &AppHandle) -> Option<Catalog> {
    let db = database_dir(app)?;
    let lang = data_language(app, None);
    catalog_from_db(&db, &lang)
}

/// 与 `AppHandle` 解耦的入口，供真机测试直接喂数据库目录。
pub(crate) fn catalog_from_db(db: &str, lang: &str) -> Option<Catalog> {
    let names: Value = std::fs::read_to_string(
        Path::new(&db).join("locales").join("global").join(format!("{}.json", lang)),
    )
    .ok()
    .and_then(|t| serde_json::from_str(&t).ok())
    .unwrap_or(Value::Null);
    let items: Value = std::fs::read_to_string(Path::new(&db).join("templates").join("items.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())?;
    let map = items.as_object()?;

    let nodes: HashSet<&str> = map
        .iter()
        .filter(|(_, v)| v.get("_type").and_then(|t| t.as_str()) == Some("Node"))
        .map(|(k, _)| k.as_str())
        .collect();

    // 父 → 子（保留顺序无所谓，分类名会排序）
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    for (id, v) in map {
        if let Some(p) = v.get("_parent").and_then(|p| p.as_str()) {
            if map.contains_key(p) {
                children.entry(p).or_default().push(id.as_str());
            }
        }
    }

    // 子树物品数（只数非 Node 叶子）
    let mut counts: HashMap<&str, usize> = HashMap::new();
    fn count_under<'a>(
        id: &'a str,
        children: &HashMap<&'a str, Vec<&'a str>>,
        nodes: &HashSet<&'a str>,
        memo: &mut HashMap<&'a str, usize>,
        depth: u8,
    ) -> usize {
        if let Some(n) = memo.get(id) {
            return *n;
        }
        if depth > 40 {
            return 0;
        }
        let mut n = 0;
        for c in children.get(id).map(|v| v.as_slice()).unwrap_or(&[]) {
            n += if nodes.contains(c) {
                count_under(c, children, nodes, memo, depth + 1)
            } else {
                1
            };
        }
        memo.insert(id, n);
        n
    }
    for id in &nodes {
        let n = count_under(id, &children, &nodes, &mut counts, 0);
        counts.insert(id, n);
    }

    // 分类表：给出所有「子树里有物品」的 Node 节点，带相对 ROOT 的层级。
    // 层级不设上限 —— 武器下面还有手枪/冲锋枪/突击步枪这类三级分类，前端按树折叠。
    let mut depth_of: HashMap<&str, u8> = HashMap::new();
    for id in &nodes {
        let mut depth = 0u8;
        let mut cur = map.get(*id).and_then(|v| v.get("_parent")).and_then(|p| p.as_str());
        let mut guard = 0;
        while let Some(p) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            if p == ROOT {
                depth += 1;
                break;
            }
            if nodes.contains(p) {
                depth += 1;
            }
            cur = map.get(p).and_then(|x| x.get("_parent")).and_then(|x| x.as_str());
        }
        if depth > 0 {
            depth_of.insert(id, depth);
        }
    }

    let mut categories: Vec<CatalogCategory> = Vec::new();
    for id in &nodes {
        let n = counts.get(id).copied().unwrap_or(0);
        let depth = match depth_of.get(id) {
            Some(d) if n > 0 => *d,
            _ => continue,
        };
        let parent = map
            .get(*id)
            .and_then(|v| v.get("_parent"))
            .and_then(|p| p.as_str())
            .filter(|p| nodes.contains(p))
            .map(|p| p.to_string());
        // 少数节点在语言包里没有中文名，退回库里的内部名（如 Weapon），再退回 id —— 分类不该出现空行
        let name = {
            let n = name_of(&names, id);
            if n.is_empty() {
                map.get(*id)
                    .and_then(|v| v.get("_name"))
                    .and_then(|x| x.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| id.to_string())
            } else {
                n
            }
        };
        categories.push(CatalogCategory {
            id: id.to_string(),
            name,
            parent,
            depth,
            count: n,
        });
    }
    categories.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then(b.count.cmp(&a.count))
            .then(a.name.cmp(&b.name))
    });

    // 物品行
    let mut rows: Vec<CatalogItem> = Vec::with_capacity(4600);
    let mut orphans = 0usize;
    for (tpl, v) in map {
        if nodes.contains(tpl.as_str()) {
            continue;
        }
        let props = v.get("_props");
        // 上溯收集 Node 祖先链（近 → 远）
        let mut chain: Vec<&str> = Vec::new();
        let mut cur = v.get("_parent").and_then(|p| p.as_str());
        let mut guard = 0;
        while let Some(p) = cur {
            guard += 1;
            if guard > 64 {
                break;
            }
            if nodes.contains(p) {
                chain.push(p);
                if p == ROOT {
                    break;
                }
            }
            cur = map.get(p).and_then(|x| x.get("_parent")).and_then(|x| x.as_str());
        }
        let leaf = match chain.first() {
            Some(l) => (*l).to_string(),
            None => {
                orphans += 1;
                continue;
            }
        };
        let name = {
            let n = name_of(&names, tpl);
            if n.is_empty() {
                v.get("_name").and_then(|x| x.as_str()).unwrap_or("").to_string()
            } else {
                n
            }
        };
        rows.push(CatalogItem {
            tpl: tpl.clone(),
            name,
            short: name_of(&names, &format!("{} ShortName", tpl)),
            leaf,
            w: as_i64(props.and_then(|p| p.get("Width"))),
            h: as_i64(props.and_then(|p| p.get("Height"))),
            stack: as_i64(props.and_then(|p| p.get("StackMaxSize"))).max(1),
            price: as_i64(props.and_then(|p| p.get("Price"))),
            rarity: props
                .and_then(|p| p.get("Rarity"))
                .and_then(|r| r.as_str())
                .unwrap_or("")
                .to_string(),
        });
    }
    rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.tpl.cmp(&b.tpl)));

    Some(Catalog {
        categories,
        items: rows,
        orphans,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 真机自检：分类树与物品归位都成立（默认忽略，需要本机 SPT 安装）
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_catalog_has_categories_and_items() {
        let cat = catalog_from_db(r"E:\EFT\SPT_Runtime\SPT_Data\database", "ch").expect("读不到库");
        let level1 = cat.categories.iter().filter(|c| c.depth == 1).count();
        assert_eq!(level1, 12, "一级分类应为 12 个，实际 {}", level1);
        assert!(cat.categories.len() >= 100, "分类过少：{}", cat.categories.len());
        assert!(cat.items.len() > 4000, "物品过少：{}", cat.items.len());
        assert_eq!(cat.orphans, 0, "有 {} 个物品没归到分类", cat.orphans);
        assert!(cat.categories.iter().all(|c| !c.name.is_empty()), "分类名不能为空");

        // M4A1：叶子分类应为「武器」，其祖先链上要有「组合物品」
        let m4 = cat
            .items
            .iter()
            .find(|i| i.tpl == "5447a9cd4bdc2dbd208b4567")
            .expect("M4A1 缺失");
        let leaf = cat.categories.iter().find(|c| c.id == m4.leaf).expect("叶子分类缺失");
        assert_eq!(leaf.name, "突击步枪");
        let mut ancestors: Vec<String> = Vec::new();
        let mut missing: Vec<String> = Vec::new();
        let mut cur = leaf.parent.clone();
        let mut guard = 0;
        while let Some(p) = cur {
            guard += 1;
            assert!(guard < 32, "祖先链成环");
            // 走到根节点就停：ROOT 本身不作为可选项展示
            if p == ROOT {
                break;
            }
            match cat.categories.iter().find(|c| c.id == p) {
                Some(c) => {
                    ancestors.push(c.name.clone());
                    cur = c.parent.clone();
                }
                None => {
                    missing.push(p.clone());
                    break;
                }
            }
        }
        assert!(missing.is_empty(), "祖先链断在 {:?}（M4A1 leaf={}）", missing, m4.leaf);
        assert!(ancestors.contains(&"武器".to_string()), "祖先链 {:?}", ancestors);
        assert!(ancestors.contains(&"组合物品".to_string()), "祖先链 {:?}", ancestors);
        assert!(m4.name.contains("M4A1"), "名称异常：{}", m4.name);
        assert!(m4.w > 0 && m4.h > 0, "尺寸异常：{}x{}", m4.w, m4.h);

        // 每个物品的叶子分类都必须存在于分类表，且能上溯到某个一级分类
        let ids: HashSet<&str> = cat.categories.iter().map(|c| c.id.as_str()).collect();
        for it in &cat.items {
            assert!(ids.contains(it.leaf.as_str()), "{} 的叶子分类不在表里", it.tpl);
        }
    }

    /// 序列化体积自检：决定前端是否必须分页
    #[test]
    #[ignore = "需要本机 SPT 安装 E:\\EFT"]
    fn real_catalog_json_size() {
        let cat = catalog_from_db(r"E:\EFT\SPT_Runtime\SPT_Data\database", "ch").expect("读不到库");
        let json = serde_json::to_string(&cat).unwrap();
        let rows = cat.items.len();
        println!("items={} bytes={} per_row={}", rows, json.len(), json.len() / rows.max(1));
        assert!(json.len() < 4_000_000, "序列化过大：{} 字节", json.len());
    }
}
