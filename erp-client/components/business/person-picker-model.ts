export type PersonPickerDepartment = Readonly<{
    id: string
    name: string
    parentId?: string | null
}>

export type PersonPickerPerson = Readonly<{
    id: string
    name: string
    departmentId?: string | null
    /** 职务、账号等用于区分同名人员的次要信息。 */
    description?: string
    disabled?: boolean
    disabledReason?: string
}>

export type PersonPickerBranch = {
    id: string
    name: string
    children: PersonPickerBranch[]
    people: PersonPickerPerson[]
}

/** 保留输入顺序；缺失上级的部门作为根节点，循环关系断开为根节点。 */
export function buildPersonPickerModel(
    departments: readonly PersonPickerDepartment[],
    people: readonly PersonPickerPerson[],
    unassignedDepartmentLabel = "未分配部门",
) {
    const units = new Map(
        departments.map((department) => [department.id, department]),
    )
    const nodes = new Map<string, PersonPickerBranch>(
        [...units.values()].map((department) => [
            department.id,
            {
                id: department.id,
                name: department.name,
                children: [],
                people: [],
            },
        ]),
    )
    const roots: PersonPickerBranch[] = []
    for (const unit of units.values()) {
        const node = nodes.get(unit.id)!
        const visited = new Set([unit.id])
        let ancestor = unit.parentId
        while (ancestor && units.has(ancestor) && !visited.has(ancestor)) {
            visited.add(ancestor)
            ancestor = units.get(ancestor)?.parentId
        }
        const parent = unit.parentId ? nodes.get(unit.parentId) : undefined
        if (parent && !(ancestor && visited.has(ancestor)))
            parent.children.push(node)
        else roots.push(node)
    }

    const byId = new Map(people.map((person) => [person.id, person]))
    const unassigned: PersonPickerPerson[] = []
    const unknownDepartment: PersonPickerPerson[] = []
    for (const person of byId.values()) {
        const department = person.departmentId
            ? nodes.get(person.departmentId)
            : undefined
        if (department) department.people.push(person)
        else if (person.departmentId) unknownDepartment.push(person)
        else unassigned.push(person)
    }

    const paths = new Map<string, string>()
    const visit = (node: PersonPickerBranch, ancestors: string[]) => {
        const path = [...ancestors, node.name]
        paths.set(node.id, path.join(" / "))
        node.children.forEach((child) => visit(child, path))
    }
    roots.forEach((node) => visit(node, []))
    const departmentLabel = (person: PersonPickerPerson) =>
        person.departmentId
            ? (paths.get(person.departmentId) ?? "部门信息不可用")
            : unassignedDepartmentLabel

    return { roots, byId, unassigned, unknownDepartment, departmentLabel }
}

/** 部门命中保留整棵子树，人员命中保留其祖先路径。 */
export function filterPersonPickerModel(
    roots: PersonPickerBranch[],
    query: string,
): PersonPickerBranch[] {
    const keyword = query.trim().toLocaleLowerCase()
    if (!keyword) return roots
    const prune = (node: PersonPickerBranch): PersonPickerBranch | null => {
        if (node.name.toLocaleLowerCase().includes(keyword)) return node
        const children = node.children
            .map(prune)
            .filter((child) => child !== null)
        const people = node.people.filter((person) =>
            personMatchesQuery(person, keyword),
        )
        return children.length || people.length
            ? { ...node, children, people }
            : null
    }
    return roots.map(prune).filter((node) => node !== null)
}

export function personMatchesQuery(
    person: PersonPickerPerson,
    query: string,
): boolean {
    return `${person.name} ${person.description ?? ""}`
        .toLocaleLowerCase()
        .includes(query.trim().toLocaleLowerCase())
}
