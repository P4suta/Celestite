import Std

set_option autoImplicit false

namespace Archive

abbrev Parents := Nat → List Nat

def Ordered (parents : Parents) (count : Nat) : Prop :=
  ∀ child, child < count → ∀ parent ∈ parents child, parent < child

def extend (parents : Parents) (count : Nat) (older : List Nat) : Parents :=
  fun child => if child = count then older else parents child

theorem extension_preserves_order
    (parents : Parents) (count : Nat) (older : List Nat)
    (valid : Ordered parents count)
    (known : ∀ parent ∈ older, parent < count) :
    Ordered (extend parents count older) (count + 1) := by
  intro child childBound parent member
  by_cases last : child = count
  · subst child
    simpa [extend] using known parent (by simpa [extend] using member)
  · have before : child < count := by omega
    exact valid child before parent (by simpa [extend, last] using member)

inductive Reaches (parents : Parents) (count : Nat) : Nat → Nat → Prop
  | edge {child parent : Nat} : child < count → parent ∈ parents child → Reaches parents count child parent
  | trans {first middle last : Nat} : Reaches parents count first middle → Reaches parents count middle last → Reaches parents count first last

theorem ancestry_decreases
    (parents : Parents) (count child parent : Nat)
    (valid : Ordered parents count)
    (path : Reaches parents count child parent) : parent < child := by
  induction path with
  | edge bound member => exact valid _ bound _ member
  | trans _ _ left right => exact Nat.lt_trans right left

theorem history_has_no_cycles
    (parents : Parents) (count child : Nat)
    (valid : Ordered parents count) : ¬ Reaches parents count child child := by
  intro path
  exact Nat.lt_irrefl child (ancestry_decreases parents count child child valid path)

theorem append_preserves_prior_edges
    (parents : Parents) (count child : Nat) (older : List Nat)
    (bound : child < count) : extend parents count older child = parents child := by
  have different : child ≠ count := by omega
  simp [extend, different]

def admitExtension (fresh known complete : Bool) : Bool := fresh && known && complete

inductive Choice where
  | shared | left | right | conflict
  deriving DecidableEq, Repr

def mergeChoice (same leftBase rightBase : Bool) : Choice :=
  if same then .shared else if leftBase then .right else if rightBase then .left else .conflict

theorem conflict_preserves_both
    (α : Type) (left right : List α) (item : α) :
    item ∈ left ∨ item ∈ right → item ∈ left ++ right := by
  intro member
  exact List.mem_append.mpr member

#print axioms extension_preserves_order
#print axioms ancestry_decreases
#print axioms history_has_no_cycles
#print axioms append_preserves_prior_edges
#print axioms conflict_preserves_both

end Archive
